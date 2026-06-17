//! Email triggers (IMAP & POP3). A background worker polls each active
//! `trigger.email` node's mailbox and enqueues one job per NEW message received
//! after activation. Per-trigger state (`flow.email_trigger_state`) avoids
//! re-firing and avoids mutating the mailbox.

use std::time::Duration;

use mail_parser::MessageParser;
use native_tls::TlsConnector as NativeTlsConnector;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_native_tls::TlsConnector;
use uuid::Uuid;

use crate::models::workflow::{WorkflowDefinition, WorkflowNode};
use crate::runtime::queue;
use crate::state::AppState;

const POLL_INTERVAL_SECS: u64 = 60;
const MAX_PER_POLL: usize = 20;

pub fn spawn(state: AppState) {
    tokio::spawn(async move { email_loop(state).await });
}

struct EmailCfg {
    protocol: String,
    host:     String,
    port:     u16,
    user:     String,
    pass:     String,
    secure:   bool,
    folder:   String,
}

async fn email_loop(state: AppState) {
    tracing::info!("Flow : worker de déclencheurs e-mail (IMAP/POP3) démarré");
    loop {
        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;

        let active = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
            "SELECT id, owner_id, file_id FROM flow.workflows WHERE status = 'active' AND is_trashed = FALSE",
        )
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for (wf_id, owner_id, file_id) in active {
            let def_val = match file_id {
                Some(fid) => crate::services::content_files::read_definition(&state, owner_id, fid).await
                    .unwrap_or_else(|_| crate::services::content_files::empty_definition()),
                None => continue,
            };
            let def = WorkflowDefinition::from_value(&def_val);
            for node in def.nodes.iter().filter(|n| n.node_type == "trigger.email") {
                if let Err(e) = poll_one(&state, wf_id, owner_id, node).await {
                    tracing::warn!(workflow = %wf_id, error = %e, "Déclencheur e-mail : échec du polling");
                }
            }
        }
    }
}

async fn resolve_cfg(state: &AppState, owner: Uuid, node: &WorkflowNode) -> Result<EmailCfg, String> {
    let protocol = node.config.get("protocol").and_then(|v| v.as_str()).unwrap_or("imap").to_string();
    let folder = node.config.get("folder").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("INBOX").to_string();

    // Credential (imap) prioritaire, sinon champs directs.
    let (host, port_opt, user, pass, secure) = if let Some(id) = node.config.get("credential").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        let cid = Uuid::parse_str(id.trim()).map_err(|_| "credential invalide".to_string())?;
        let data = crate::services::credentials::resolve(&state.db, &state.settings.core.internal_secret, owner, cid).await?;
        let g = |k: &str| data.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let port = data.get("port").and_then(|v| v.as_i64())
            .or_else(|| data.get("port").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()));
        (g("host"), port.map(|p| p as u16), g("user"), g("password"), data.get("secure").and_then(|v| v.as_bool()).unwrap_or(true))
    } else {
        let g = |k: &str| node.config.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let port = node.config.get("port").and_then(|v| v.as_i64()).map(|p| p as u16);
        (g("host"), port, g("username"), g("password"), node.config.get("secure").and_then(|v| v.as_bool()).unwrap_or(true))
    };

    if host.is_empty() || user.is_empty() {
        return Err("hôte/utilisateur manquant".into());
    }
    let port = port_opt.unwrap_or(match (protocol.as_str(), secure) {
        ("pop3", true) => 995, ("pop3", false) => 110,
        (_, false) => 143, _ => 993,
    });
    Ok(EmailCfg { protocol, host, port, user, pass, secure, folder })
}

async fn poll_one(state: &AppState, wf_id: Uuid, owner: Uuid, node: &WorkflowNode) -> Result<(), String> {
    let cfg = resolve_cfg(state, owner, node).await?;
    let prev: Option<Value> = sqlx::query_scalar(
        "SELECT state FROM flow.email_trigger_state WHERE workflow_id = $1 AND node_id = $2",
    )
    .bind(wf_id).bind(&node.id)
    .fetch_optional(&state.db).await.map_err(|e| e.to_string())?;

    let (messages, new_state) = if cfg.protocol == "pop3" {
        let seen: Vec<String> = prev.as_ref().and_then(|s| s.get("seen")).and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let first = prev.is_none();
        let (msgs, new_seen) = pop3_poll(&cfg, &seen, first).await?;
        (msgs, json!({ "seen": new_seen }))
    } else {
        let last_uid: Option<u32> = prev.as_ref().and_then(|s| s.get("last_uid")).and_then(|v| v.as_u64()).map(|u| u as u32);
        let (msgs, new_last) = imap_poll(&cfg, last_uid).await?;
        (msgs, json!({ "last_uid": new_last }))
    };

    for m in messages {
        let trigger_data = json!({ "email": m });
        let _ = queue::enqueue(&state.db, wf_id, owner, "email", trigger_data, state.settings.runtime.max_retries).await;
    }

    sqlx::query(
        r#"INSERT INTO flow.email_trigger_state (workflow_id, node_id, state, updated_at)
           VALUES ($1, $2, $3, NOW())
           ON CONFLICT (workflow_id, node_id) DO UPDATE SET state = $3, updated_at = NOW()"#,
    )
    .bind(wf_id).bind(&node.id).bind(&new_state)
    .execute(&state.db).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Parse une RFC 5322 brute en objet JSON exploitable par le workflow.
fn parse_email(raw: &[u8]) -> Value {
    let parser = MessageParser::default();
    let Some(p) = parser.parse(raw) else {
        return json!({ "raw": String::from_utf8_lossy(raw) });
    };
    let from = p.from().and_then(|a| a.first()).map(|addr| json!({
        "name": addr.name(), "email": addr.address().unwrap_or("")
    }));
    let to: Vec<Value> = p.to().map(|a| a.iter().map(|addr| json!({ "name": addr.name(), "email": addr.address().unwrap_or("") })).collect()).unwrap_or_default();
    json!({
        "from":       from,
        "to":         to,
        "subject":    p.subject().unwrap_or(""),
        "date":       p.date().map(|d| d.to_rfc3339()),
        "message_id": p.message_id(),
        "text":       p.body_text(0).map(|s| s.into_owned()),
        "html":       p.body_html(0).map(|s| s.into_owned()),
    })
}

// ── IMAP ─────────────────────────────────────────────────────────────────────────

async fn imap_poll(cfg: &EmailCfg, last_uid: Option<u32>) -> Result<(Vec<Value>, u32), String> {
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let tcp = tokio::time::timeout(Duration::from_secs(20), TcpStream::connect(&addr)).await
        .map_err(|_| "IMAP : délai de connexion".to_string())?
        .map_err(|e| format!("IMAP TCP : {e}"))?;

    if cfg.secure {
        let native = NativeTlsConnector::new().map_err(|e| format!("TLS : {e}"))?;
        let tls = TlsConnector::from(native).connect(&cfg.host, tcp).await.map_err(|e| format!("TLS : {e}"))?;
        let session = async_imap::Client::new(tls).login(&cfg.user, &cfg.pass).await.map_err(|(e, _)| format!("IMAP login : {e}"))?;
        imap_fetch(session, &cfg.folder, last_uid).await
    } else {
        let session = async_imap::Client::new(tcp).login(&cfg.user, &cfg.pass).await.map_err(|(e, _)| format!("IMAP login : {e}"))?;
        imap_fetch(session, &cfg.folder, last_uid).await
    }
}

async fn imap_fetch<T>(mut session: async_imap::Session<T>, folder: &str, last_uid: Option<u32>) -> Result<(Vec<Value>, u32), String>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    use futures::TryStreamExt;
    let mailbox = session.select(folder).await.map_err(|e| format!("IMAP SELECT : {e}"))?;
    let max_uid = mailbox.uid_next.map(|n| n.saturating_sub(1)).unwrap_or(0);

    let Some(last) = last_uid else {
        // Premier passage : on mémorise l'état courant, on ne déclenche rien (pas de backlog).
        let _ = session.logout().await;
        return Ok((Vec::new(), max_uid));
    };

    let mut out = Vec::new();
    let mut new_last = last;
    if max_uid > last {
        let set = format!("{}:*", last + 1);
        let fetches = session.uid_fetch(&set, "(UID BODY.PEEK[])").await.map_err(|e| format!("IMAP FETCH : {e}"))?;
        let mut items: Vec<(u32, Vec<u8>)> = fetches.try_collect::<Vec<_>>().await.map_err(|e| format!("IMAP collect : {e}"))?
            .into_iter()
            .filter_map(|m| Some((m.uid?, m.body()?.to_vec())))
            .filter(|(uid, _)| *uid > last)
            .collect();
        items.sort_by_key(|(uid, _)| *uid);
        items.truncate(MAX_PER_POLL);
        for (uid, body) in items {
            new_last = new_last.max(uid);
            out.push(parse_email(&body));
        }
        if new_last < max_uid { new_last = max_uid; } // ne pas re-balayer les messages tronqués
    }
    let _ = session.logout().await;
    Ok((out, new_last))
}

// ── POP3 (client minimal) ────────────────────────────────────────────────────────

async fn pop3_poll(cfg: &EmailCfg, seen: &[String], first_run: bool) -> Result<(Vec<Value>, Vec<String>), String> {
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let tcp = tokio::time::timeout(Duration::from_secs(20), TcpStream::connect(&addr)).await
        .map_err(|_| "POP3 : délai de connexion".to_string())?
        .map_err(|e| format!("POP3 TCP : {e}"))?;
    if cfg.secure {
        let native = NativeTlsConnector::new().map_err(|e| format!("TLS : {e}"))?;
        let tls = TlsConnector::from(native).connect(&cfg.host, tcp).await.map_err(|e| format!("TLS : {e}"))?;
        pop3_session(tls, cfg, seen, first_run).await
    } else {
        pop3_session(tcp, cfg, seen, first_run).await
    }
}

async fn pop3_session<S>(stream: S, cfg: &EmailCfg, seen: &[String], first_run: bool) -> Result<(Vec<Value>, Vec<String>), String>
where
    S: tokio::io::AsyncRead + AsyncWrite + Unpin,
{
    let mut io = BufReader::new(stream);
    read_status(&mut io).await?;                 // greeting
    send(&mut io, &format!("USER {}", cfg.user)).await?; read_status(&mut io).await?;
    send(&mut io, &format!("PASS {}", cfg.pass)).await?; read_status(&mut io).await?;

    // UIDL → liste (num, uidl).
    send(&mut io, "UIDL").await?; read_status(&mut io).await?;
    let lines = read_multiline(&mut io).await?;
    let mut listing: Vec<(u32, String)> = Vec::new();
    for l in lines {
        let mut it = l.split_whitespace();
        if let (Some(n), Some(uid)) = (it.next(), it.next()) {
            if let Ok(num) = n.parse::<u32>() { listing.push((num, uid.to_string())); }
        }
    }
    let all_uids: Vec<String> = listing.iter().map(|(_, u)| u.clone()).collect();

    let mut out = Vec::new();
    if !first_run {
        let news: Vec<(u32, String)> = listing.into_iter().filter(|(_, u)| !seen.contains(u)).take(MAX_PER_POLL).collect();
        for (num, _uid) in news {
            send(&mut io, &format!("RETR {num}")).await?;
            read_status(&mut io).await?;
            let body = read_multiline_raw(&mut io).await?;
            out.push(parse_email(&body));
        }
    }
    let _ = send(&mut io, "QUIT").await;

    // Mémoire des UIDL vus (cap 500).
    let mut new_seen: Vec<String> = all_uids;
    if new_seen.len() > 500 { new_seen = new_seen.split_off(new_seen.len() - 500); }
    Ok((out, new_seen))
}

async fn send<S: tokio::io::AsyncRead + AsyncWrite + Unpin>(io: &mut BufReader<S>, cmd: &str) -> Result<(), String> {
    io.get_mut().write_all(format!("{cmd}\r\n").as_bytes()).await.map_err(|e| format!("POP3 write : {e}"))?;
    io.get_mut().flush().await.map_err(|e| format!("POP3 flush : {e}"))
}

async fn read_status<S: tokio::io::AsyncRead + Unpin>(io: &mut BufReader<S>) -> Result<String, String> {
    let mut line = String::new();
    io.read_line(&mut line).await.map_err(|e| format!("POP3 read : {e}"))?;
    if line.starts_with("+OK") { Ok(line) } else { Err(format!("POP3 : {}", line.trim())) }
}

/// Lit une réponse multiligne textuelle jusqu'à la ligne « . », dé-stuffe les points.
async fn read_multiline<S: tokio::io::AsyncRead + Unpin>(io: &mut BufReader<S>) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    loop {
        let mut line = String::new();
        if io.read_line(&mut line).await.map_err(|e| format!("POP3 read : {e}"))? == 0 { break; }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "." { break; }
        out.push(trimmed.strip_prefix("..").map(|s| format!(".{s}")).unwrap_or_else(|| trimmed.to_string()));
    }
    Ok(out)
}

/// Idem mais retourne les octets bruts (pour parser un message RETR).
async fn read_multiline_raw<S: tokio::io::AsyncRead + Unpin>(io: &mut BufReader<S>) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    loop {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        // lecture ligne à ligne par octet (suffisant, volume borné par MAX_PER_POLL).
        loop {
            let n = io.read(&mut byte).await.map_err(|e| format!("POP3 read : {e}"))?;
            if n == 0 { break; }
            line.push(byte[0]);
            if byte[0] == b'\n' { break; }
        }
        if line.is_empty() { break; }
        let trimmed: &[u8] = line.strip_suffix(b"\r\n").or_else(|| line.strip_suffix(b"\n")).unwrap_or(&line);
        if trimmed == b"." { break; }
        let content = trimmed.strip_prefix(b"..").map(|r| { let mut v = vec![b'.']; v.extend_from_slice(r); v }).unwrap_or_else(|| trimmed.to_vec());
        buf.extend_from_slice(&content);
        buf.push(b'\n');
    }
    Ok(buf)
}
