//! Generic, data-driven integration nodes. One executor (`ServiceNode`) drives a
//! large catalogue of popular services (`service_catalog`): each entry declares a
//! base URL, an auth style, a credential type and a set of operations (method +
//! path). At runtime the node builds the HTTP request, applies auth from the
//! resolved credential and calls the service through `CoreProxy.call_external`.
//!
//! Auth: API-key / token / Basic services work directly. OAuth2 services (Google,
//! Microsoft…) authenticate via a pasted Bearer access token (no OAuth flow here).

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::nodes::trait_::{
    ExecutionContext, FieldDef, FieldType, NodeCategory, NodeContext, NodeError, NodeMeta, NodeOutput,
};

#[derive(Clone, Copy)]
pub enum Auth { Bearer, Header(&'static str), Basic, Query(&'static str), None }

#[derive(Clone)]
pub struct Op { pub id: &'static str, pub label: &'static str, pub method: &'static str, pub path: &'static str, pub body: bool }

#[derive(Clone)]
pub struct Svc {
    pub type_id: &'static str,
    pub name:    &'static str,
    pub icon:    &'static str,
    pub color:   &'static str,
    pub base:    &'static str,   // racine API ; suffixe si `instance`
    pub instance: bool,          // base URL = host du credential + `base`
    pub cred:    &'static str,   // types de credential acceptés (CSV)
    pub auth:    Auth,
    pub ops:     Vec<Op>,
}

fn op(id: &'static str, label: &'static str, method: &'static str, path: &'static str, body: bool) -> Op {
    Op { id, label, method, path, body }
}

pub struct ServiceNode(pub Svc);

#[async_trait]
impl crate::nodes::trait_::NodeExecutor for ServiceNode {
    fn meta(&self) -> NodeMeta {
        let s = &self.0;
        let opts: Vec<(&str, &str)> = s.ops.iter().map(|o| (o.id, o.label)).collect();
        NodeMeta {
            node_type: s.type_id.into(),
            name:      s.name.into(),
            description: format!("Intégration {} — {} opérations", s.name, s.ops.len()),
            category:  NodeCategory::Integration,
            icon:      s.icon.into(),
            color:     s.color.into(),
            inputs: 1, outputs: vec![],
            fields: vec![
                FieldDef::credential("credential", "Credential", s.cred),
                FieldDef::new("operation", "Opération", FieldType::Select).required()
                    .options(&opts).default(json!(s.ops.first().map(|o| o.id).unwrap_or(""))),
                FieldDef::new("id", "Identifiant / ressource ({id})", FieldType::Expression)
                    .help("Remplace {id} dans le chemin (ex. ID de message, de ligne…)."),
                FieldDef::new("query", "Paramètres d'URL", FieldType::Text).placeholder("limit=10&type=x"),
                FieldDef::new("body", "Corps (JSON)", FieldType::Json).help("Pour les opérations d'écriture (créer/mettre à jour)."),
            ],
        }
    }

    async fn execute(&self, config: Value, _ctx: &ExecutionContext, n: &NodeContext<'_>) -> Result<NodeOutput, NodeError> {
        let s = &self.0;
        let op_id = config.get("operation").and_then(|v| v.as_str()).unwrap_or_else(|| s.ops.first().map(|o| o.id).unwrap_or(""));
        let opn = s.ops.iter().find(|o| o.id == op_id).or_else(|| s.ops.first())
            .ok_or(NodeError::InvalidConfig("Aucune opération".into()))?;

        let cred = config.get("credential").cloned().unwrap_or(Value::Null);

        // URL de base (host du credential pour les services auto-hébergés).
        let base = if s.instance {
            let host = cval(&cred, &["host", "url", "instanceUrl", "domain", "baseUrl", "server"]);
            if host.is_empty() { return Err(NodeError::MissingField("host (credential)")); }
            format!("{}{}", host.trim_end_matches('/'), s.base)
        } else {
            s.base.to_string()
        };

        // Chemin + substitution de {id}.
        let id_val = config.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let path = opn.path.replace("{id}", id_val);
        let mut url = format!("{base}{path}");
        if let Some(q) = config.get("query").and_then(|v| v.as_str()).filter(|q| !q.is_empty()) {
            let sep = if url.contains('?') { '&' } else { '?' };
            url.push_str(&format!("{sep}{q}"));
        }

        let mut headers: HashMap<String, String> = HashMap::new();
        apply_auth(&s.auth, &cred, &mut headers, &mut url);
        // En-tête spécifique Nextcloud OCS.
        if s.type_id == "svc.nextcloud" { headers.insert("OCS-APIRequest".into(), "true".into()); headers.insert("Accept".into(), "application/json".into()); }

        let method = opn.method.parse::<Method>().unwrap_or(Method::GET);
        let body = if opn.body { config.get("body").filter(|v| !v.is_null()).cloned() } else { None };

        let resp = n.proxy.call_external(&url, method, headers, body, 60, n.user_id)
            .await.map_err(|e| NodeError::ProxyError(e.to_string()))?;
        Ok(NodeOutput::data(json!({ "status": resp.status, "body": resp.body })))
    }
}

/// Première valeur non vide parmi `keys` du credential.
fn cval(cred: &Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(s) = cred.get(*k).and_then(|v| v.as_str()) {
            if !s.is_empty() { return s.to_string(); }
        }
    }
    String::new()
}

fn apply_auth(auth: &Auth, cred: &Value, headers: &mut HashMap<String, String>, url: &mut String) {
    use base64::Engine;
    match auth {
        Auth::Bearer => {
            let t = cval(cred, &["token", "accessToken", "apiKey", "apiToken", "botToken", "secretKey", "value"]);
            if !t.is_empty() { headers.insert("Authorization".into(), format!("Bearer {t}")); }
        }
        Auth::Header(name) => {
            let v = cval(cred, &["value", "apiKey", "token", "accessToken", "apiToken", "secretKey", "serviceRole", "botToken"]);
            if !v.is_empty() { headers.insert((*name).to_string(), v); }
        }
        Auth::Basic => {
            let u = cval(cred, &["username", "user", "email", "accountSid", "consumerKey", "apiKey", "applicationId"]);
            let p = cval(cred, &["password", "appPassword", "apiToken", "authToken", "apiSecret", "consumerSecret", "apiKey", "secretKey"]);
            let enc = base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"));
            headers.insert("Authorization".into(), format!("Basic {enc}"));
        }
        Auth::Query(name) => {
            let v = cval(cred, &["apiKey", "token", "value", "key", "accessToken"]);
            if !v.is_empty() {
                let sep = if url.contains('?') { '&' } else { '?' };
                url.push_str(&format!("{sep}{name}={v}"));
            }
        }
        Auth::None => {}
    }
}

// ── Catalogue (~100 services) ────────────────────────────────────────────────────

#[allow(clippy::vec_init_then_push)]
pub fn service_catalog() -> Vec<Svc> {
    use Auth::*;
    // Constructeur terse.
    #[allow(clippy::too_many_arguments)]
    fn s(type_id: &'static str, name: &'static str, icon: &'static str, color: &'static str, base: &'static str, cred: &'static str, auth: Auth, ops: Vec<Op>) -> Svc {
        Svc { type_id, name, icon, color, base, instance: false, cred, auth, ops }
    }
    #[allow(clippy::too_many_arguments)]
    fn si(type_id: &'static str, name: &'static str, icon: &'static str, color: &'static str, base: &'static str, cred: &'static str, auth: Auth, ops: Vec<Op>) -> Svc {
        Svc { type_id, name, icon, color, base, instance: true, cred, auth, ops }
    }
    // Opérations CRUD génériques sur une ressource REST.
    fn crud(res: &'static str) -> Vec<Op> {
        vec![
            op("list", "Lister", "GET", res, false),
            op("get", "Récupérer", "GET", leak(format!("{res}/{{id}}")), false),
            op("create", "Créer", "POST", res, true),
            op("update", "Mettre à jour", "PATCH", leak(format!("{res}/{{id}}")), true),
            op("delete", "Supprimer", "DELETE", leak(format!("{res}/{{id}}")), false),
        ]
    }
    let mut v: Vec<Svc> = Vec::new();

    // ── Google (jeton Bearer) ──
    v.push(s("svc.gmail", "Gmail", "Mail", "#ea4335", "https://gmail.googleapis.com/gmail/v1/users/me", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("send", "Envoyer (raw)", "POST", "/messages/send", true),
        op("listMessages", "Lister messages", "GET", "/messages", false),
        op("getMessage", "Lire message", "GET", "/messages/{id}", false),
        op("listLabels", "Lister labels", "GET", "/labels", false),
        op("listThreads", "Lister fils", "GET", "/threads", false),
        op("getThread", "Lire fil", "GET", "/threads/{id}", false),
        op("modifyMessage", "Modifier labels (message)", "POST", "/messages/{id}/modify", true),
        op("trashMessage", "Mettre à la corbeille", "POST", "/messages/{id}/trash", false),
        op("untrashMessage", "Restaurer", "POST", "/messages/{id}/untrash", false),
        op("deleteMessage", "Supprimer message", "DELETE", "/messages/{id}", false),
        op("createDraft", "Créer brouillon", "POST", "/drafts", true),
        op("listDrafts", "Lister brouillons", "GET", "/drafts", false),
        op("createLabel", "Créer label", "POST", "/labels", true),
        op("getProfile", "Mon profil", "GET", "/profile", false),
    ]));
    v.push(s("svc.googleDrive", "Google Drive", "Cloud", "#1fa463", "https://www.googleapis.com/drive/v3", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("list", "Lister fichiers", "GET", "/files", false),
        op("get", "Récupérer fichier", "GET", "/files/{id}", false),
        op("delete", "Supprimer", "DELETE", "/files/{id}", false),
        op("create", "Créer (metadata)", "POST", "/files", true),
        op("update", "Mettre à jour (metadata)", "PATCH", "/files/{id}", true),
        op("copy", "Copier", "POST", "/files/{id}/copy", true),
        op("export", "Exporter", "GET", "/files/{id}/export", false),
        op("listPermissions", "Lister permissions", "GET", "/files/{id}/permissions", false),
        op("createPermission", "Partager", "POST", "/files/{id}/permissions", true),
        op("about", "Compte & quota", "GET", "/about?fields=user,storageQuota", false),
    ]));
    v.push(s("svc.googleCalendar", "Google Agenda", "Calendar", "#4285f4", "https://www.googleapis.com/calendar/v3", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("listEvents", "Lister événements", "GET", "/calendars/primary/events", false),
        op("createEvent", "Créer événement", "POST", "/calendars/primary/events", true),
        op("getEvent", "Lire événement", "GET", "/calendars/primary/events/{id}", false),
        op("deleteEvent", "Supprimer événement", "DELETE", "/calendars/primary/events/{id}", false),
        op("updateEvent", "Mettre à jour événement", "PUT", "/calendars/primary/events/{id}", true),
        op("quickAdd", "Ajout rapide (texte)", "POST", "/calendars/primary/events/quickAdd", false),
        op("listCalendars", "Lister mes agendas", "GET", "/users/me/calendarList", false),
        op("freeBusy", "Disponibilités", "POST", "/freeBusy", true),
    ]));
    v.push(s("svc.googleSheets", "Google Sheets", "Table", "#0f9d58", "https://sheets.googleapis.com/v4/spreadsheets", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("get", "Lire feuille", "GET", "/{id}", false),
        op("getValues", "Lire plage", "GET", "/{id}/values/A1:Z1000", false),
        op("append", "Ajouter ligne", "POST", "/{id}/values/A1:append?valueInputOption=USER_ENTERED", true),
        op("updateValues", "Écrire plage", "PUT", "/{id}/values/A1:Z1000?valueInputOption=USER_ENTERED", true),
        op("clearValues", "Effacer plage", "POST", "/{id}/values/A1:Z1000:clear", true),
        op("batchUpdate", "Mise à jour groupée", "POST", "/{id}:batchUpdate", true),
        op("create", "Créer une feuille", "POST", "", true),
    ]));
    v.push(s("svc.googleDocs", "Google Docs", "FileText", "#4285f4", "https://docs.googleapis.com/v1/documents", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("get", "Lire document", "GET", "/{id}", false),
        op("batchUpdate", "Mettre à jour", "POST", "/{id}:batchUpdate", true),
    ]));
    v.push(s("svc.googleContacts", "Google Contacts", "Contact", "#4285f4", "https://people.googleapis.com/v1", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("list", "Lister contacts", "GET", "/people/me/connections?personFields=names,emailAddresses", false),
        op("create", "Créer contact", "POST", "/people:createContact", true),
    ]));
    v.push(s("svc.googleTasks", "Google Tasks", "ListChecks", "#4285f4", "https://tasks.googleapis.com/tasks/v1", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("listLists", "Lister listes", "GET", "/users/@me/lists", false),
        op("list", "Lister tâches", "GET", "/lists/{id}/tasks", false),
        op("create", "Créer tâche", "POST", "/lists/{id}/tasks", true),
    ]));
    v.push(s("svc.youtube", "YouTube", "Youtube", "#ff0000", "https://www.googleapis.com/youtube/v3", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("search", "Rechercher", "GET", "/search?part=snippet", false),
        op("listChannels", "Mes chaînes", "GET", "/channels?part=snippet,statistics&mine=true", false),
    ]));
    v.push(s("svc.googleMaps", "Google Maps", "Map", "#34a853", "https://maps.googleapis.com/maps/api", "googleMapsApi,apiKeyAuth", Query("key"), vec![
        op("geocode", "Géocoder", "GET", "/geocode/json", false),
        op("places", "Lieux (texte)", "GET", "/place/textsearch/json", false),
        op("directions", "Itinéraire", "GET", "/directions/json", false),
    ]));

    // ── Microsoft (jeton Bearer Graph) ──
    v.push(s("svc.outlook", "Outlook", "Mail", "#0072c6", "https://graph.microsoft.com/v1.0/me", "microsoftAccessToken,httpBearerAuth", Bearer, vec![
        op("listMessages", "Lister messages", "GET", "/messages", false),
        op("sendMail", "Envoyer", "POST", "/sendMail", true),
        op("listEvents", "Lister événements", "GET", "/events", false),
        op("getMessage", "Lire message", "GET", "/messages/{id}", false),
        op("createDraft", "Créer brouillon", "POST", "/messages", true),
        op("deleteMessage", "Supprimer message", "DELETE", "/messages/{id}", false),
        op("createEvent", "Créer événement", "POST", "/events", true),
        op("listContacts", "Lister contacts", "GET", "/contacts", false),
        op("listFolders", "Dossiers de courrier", "GET", "/mailFolders", false),
    ]));
    v.push(s("svc.oneDrive", "OneDrive", "Cloud", "#0078d4", "https://graph.microsoft.com/v1.0/me/drive", "microsoftAccessToken,httpBearerAuth", Bearer, vec![
        op("root", "Lister racine", "GET", "/root/children", false),
        op("item", "Récupérer élément", "GET", "/items/{id}", false),
    ]));
    v.push(s("svc.msTeams", "Microsoft Teams", "MessageSquare", "#6264a7", "https://graph.microsoft.com/v1.0", "microsoftAccessToken,httpBearerAuth", Bearer, vec![
        op("listTeams", "Mes équipes", "GET", "/me/joinedTeams", false),
        op("sendChannel", "Message canal", "POST", "/teams/{id}/channels", true),
    ]));
    v.push(s("svc.msExcel", "Microsoft Excel", "Table", "#217346", "https://graph.microsoft.com/v1.0/me/drive", "microsoftAccessToken,httpBearerAuth", Bearer, vec![
        op("worksheets", "Feuilles", "GET", "/items/{id}/workbook/worksheets", false),
    ]));
    v.push(s("svc.msTodo", "Microsoft To Do", "ListChecks", "#3999e5", "https://graph.microsoft.com/v1.0/me/todo", "microsoftAccessToken,httpBearerAuth", Bearer, vec![
        op("lists", "Listes", "GET", "/lists", false),
        op("tasks", "Tâches", "GET", "/lists/{id}/tasks", false),
    ]));

    // ── Nextcloud / cloud storage ──
    v.push(si("svc.nextcloud", "Nextcloud", "Cloud", "#0082c9", "/ocs/v2.php", "nextcloud", Basic, vec![
        op("user", "Mon profil", "GET", "/cloud/user", false),
        op("listUsers", "Lister utilisateurs", "GET", "/cloud/users", false),
        op("listShares", "Lister partages", "GET", "/apps/files_sharing/api/v1/shares", false),
        op("createShare", "Créer un partage", "POST", "/apps/files_sharing/api/v1/shares", true),
        op("notify", "Notification", "POST", "/apps/notifications/api/v2/admin_notifications/{id}", true),
        op("deleteShare", "Supprimer un partage", "DELETE", "/apps/files_sharing/api/v1/shares/{id}", false),
        op("groups", "Lister groupes", "GET", "/cloud/groups", false),
        op("createUser", "Créer utilisateur", "POST", "/cloud/users", true),
    ]));
    v.push(s("svc.dropbox", "Dropbox", "Cloud", "#0061ff", "https://api.dropboxapi.com/2", "dropboxApi,httpBearerAuth", Bearer, vec![
        op("listFolder", "Lister dossier", "POST", "/files/list_folder", true),
        op("getMetadata", "Métadonnées", "POST", "/files/get_metadata", true),
        op("createFolder", "Créer dossier", "POST", "/files/create_folder_v2", true),
        op("delete", "Supprimer", "POST", "/files/delete_v2", true),
    ]));
    v.push(s("svc.box", "Box", "Cloud", "#0061d5", "https://api.box.com/2.0", "boxApi,httpBearerAuth", Bearer, vec![
        op("folder", "Contenu dossier", "GET", "/folders/{id}/items", false),
        op("file", "Infos fichier", "GET", "/files/{id}", false),
    ]));

    // ── Communication / chat ──
    v.push(s("svc.slack", "Slack", "MessageSquare", "#4a154b", "https://slack.com/api", "slackApi,slackOAuth2Api,httpBearerAuth", Bearer, vec![
        op("postMessage", "Envoyer message", "POST", "/chat.postMessage", true),
        op("listChannels", "Lister canaux", "GET", "/conversations.list", false),
        op("listUsers", "Lister membres", "GET", "/users.list", false),
        op("uploadFile", "Téléverser fichier", "POST", "/files.upload", true),
        op("updateMessage", "Modifier message", "POST", "/chat.update", true),
        op("deleteMessage", "Supprimer message", "POST", "/chat.delete", true),
        op("history", "Historique du canal", "GET", "/conversations.history", false),
        op("createChannel", "Créer canal", "POST", "/conversations.create", true),
        op("invite", "Inviter au canal", "POST", "/conversations.invite", true),
        op("userInfo", "Infos utilisateur", "GET", "/users.info", false),
        op("addReaction", "Ajouter réaction", "POST", "/reactions.add", true),
        op("openDm", "Ouvrir un message privé", "POST", "/conversations.open", true),
    ]));
    v.push(s("svc.discord", "Discord", "MessageSquare", "#5865f2", "https://discord.com/api/v10", "discordApi", Header("Authorization"), vec![
        op("sendMessage", "Envoyer message", "POST", "/channels/{id}/messages", true),
        op("getChannel", "Récupérer canal", "GET", "/channels/{id}", false),
        op("getGuild", "Récupérer serveur", "GET", "/guilds/{id}", false),
        op("getMessages", "Lister messages", "GET", "/channels/{id}/messages", false),
        op("editMessage", "Modifier message", "PATCH", "/channels/{id}/messages", true),
        op("createDm", "Ouvrir un message privé", "POST", "/users/@me/channels", true),
        op("guildMembers", "Membres du serveur", "GET", "/guilds/{id}/members", false),
        op("createChannel", "Créer canal", "POST", "/guilds/{id}/channels", true),
    ]));
    v.push(s("svc.telegram", "Telegram", "Send", "#0088cc", "https://api.telegram.org", "telegramApi", None, vec![
        op("sendMessage", "Envoyer message", "POST", "/sendMessage", true),
        op("getUpdates", "Récupérer updates", "GET", "/getUpdates", false),
        op("sendPhoto", "Envoyer photo", "POST", "/sendPhoto", true),
        op("sendDocument", "Envoyer document", "POST", "/sendDocument", true),
        op("editMessageText", "Modifier message", "POST", "/editMessageText", true),
        op("deleteMessage", "Supprimer message", "POST", "/deleteMessage", true),
        op("getMe", "Infos du bot", "GET", "/getMe", false),
        op("answerCallback", "Répondre à un callback", "POST", "/answerCallbackQuery", true),
    ]));
    v.push(si("svc.mattermost", "Mattermost", "MessageSquare", "#0058cc", "/api/v4", "mattermostApi,httpBearerAuth", Bearer, vec![
        op("createPost", "Publier", "POST", "/posts", true),
        op("listChannels", "Canaux d'une équipe", "GET", "/teams/{id}/channels", false),
        op("me", "Mon profil", "GET", "/users/me", false),
    ]));
    v.push(si("svc.rocketchat", "Rocket.Chat", "MessageSquare", "#f5455c", "/api/v1", "rocketChatApi", Header("X-Auth-Token"), vec![
        op("postMessage", "Envoyer message", "POST", "/chat.postMessage", true),
        op("channelsList", "Lister canaux", "GET", "/channels.list", false),
    ]));
    v.push(s("svc.twilio", "Twilio", "Phone", "#f22f46", "https://api.twilio.com/2010-04-01/Accounts", "twilioApi", Basic, vec![
        op("sendSms", "Envoyer SMS", "POST", "/{id}/Messages.json", true),
        op("listMessages", "Lister SMS", "GET", "/{id}/Messages.json", false),
    ]));
    v.push(s("svc.vonage", "Vonage", "Phone", "#871fff", "https://rest.nexmo.com", "vonageApi", Query("api_key"), vec![
        op("sendSms", "Envoyer SMS", "POST", "/sms/json", true),
    ]));
    v.push(s("svc.whatsapp", "WhatsApp Business", "MessageSquare", "#25d366", "https://graph.facebook.com/v20.0", "whatsAppApi,httpBearerAuth", Bearer, vec![
        op("sendMessage", "Envoyer message", "POST", "/{id}/messages", true),
    ]));
    v.push(si("svc.matrix", "Matrix", "MessageSquare", "#0dbd8b", "/_matrix/client/v3", "matrixApi,httpBearerAuth", Bearer, vec![
        op("sendMessage", "Envoyer (room)", "PUT", "/rooms/{id}/send/m.room.message/1", true),
        op("joinedRooms", "Salons rejoints", "GET", "/joined_rooms", false),
    ]));
    v.push(s("svc.pushover", "Pushover", "Bell", "#249df1", "https://api.pushover.net/1", "apiKeyAuth", None, vec![
        op("push", "Notification", "POST", "/messages.json", true),
    ]));
    v.push(s("svc.ntfy", "ntfy", "Bell", "#338574", "https://ntfy.sh", "httpBearerAuth", Bearer, vec![
        op("publish", "Publier", "POST", "/{id}", true),
    ]));
    v.push(s("svc.gotify", "Gotify", "Bell", "#125b8c", "https://gotify.example/message", "apiKeyAuth", Query("token"), vec![
        op("push", "Notification", "POST", "", true),
    ]));

    // ── Email / marketing ──
    v.push(s("svc.sendgrid", "SendGrid", "Mail", "#1a82e2", "https://api.sendgrid.com/v3", "sendGridApi,httpBearerAuth", Bearer, vec![
        op("send", "Envoyer e-mail", "POST", "/mail/send", true),
        op("listTemplates", "Modèles", "GET", "/templates", false),
    ]));
    v.push(s("svc.mailgun", "Mailgun", "Mail", "#c02126", "https://api.mailgun.net/v3", "mailgunApi", Basic, vec![
        op("send", "Envoyer e-mail", "POST", "/{id}/messages", true),
    ]));
    v.push(s("svc.mailchimp", "Mailchimp", "Mail", "#ffe01b", "https://us1.api.mailchimp.com/3.0", "mailchimpApi", Basic, vec![
        op("lists", "Listes", "GET", "/lists", false),
        op("addMember", "Ajouter abonné", "POST", "/lists/{id}/members", true),
    ]));
    v.push(s("svc.brevo", "Brevo", "Mail", "#0b996e", "https://api.brevo.com/v3", "brevoApi", Header("api-key"), vec![
        op("sendEmail", "Envoyer e-mail", "POST", "/smtp/email", true),
        op("contacts", "Contacts", "GET", "/contacts", false),
        op("createContact", "Créer contact", "POST", "/contacts", true),
    ]));
    v.push(s("svc.mailjet", "Mailjet", "Mail", "#ffb800", "https://api.mailjet.com/v3.1", "mailjetApi", Basic, vec![
        op("send", "Envoyer e-mail", "POST", "/send", true),
    ]));
    v.push(s("svc.postmark", "Postmark", "Mail", "#ffde00", "https://api.postmarkapp.com", "postmarkApi", Header("X-Postmark-Server-Token"), vec![
        op("send", "Envoyer e-mail", "POST", "/email", true),
    ]));
    v.push(s("svc.resend", "Resend", "Mail", "#000000", "https://api.resend.com", "httpBearerAuth,apiKeyAuth", Bearer, vec![
        op("send", "Envoyer e-mail", "POST", "/emails", true),
    ]));
    v.push(s("svc.klaviyo", "Klaviyo", "Mail", "#232425", "https://a.klaviyo.com/api", "klaviyoApi", Header("Authorization"), vec![
        op("profiles", "Profils", "GET", "/profiles", false),
        op("events", "Événements", "POST", "/events", true),
    ]));
    v.push(si("svc.activecampaign", "ActiveCampaign", "Mail", "#356ae6", "/api/3", "activeCampaignApi", Header("Api-Token"), crud("/contacts")));
    v.push(s("svc.convertkit", "ConvertKit", "Mail", "#fb6970", "https://api.convertkit.com/v3", "apiKeyAuth", Query("api_key"), vec![
        op("subscribers", "Abonnés", "GET", "/subscribers", false),
    ]));

    // ── Dev / DevOps ──
    v.push(s("svc.github", "GitHub", "Github", "#181717", "https://api.github.com", "githubApi,githubOAuth2Api,httpBearerAuth", Bearer, vec![
        op("listRepos", "Mes dépôts", "GET", "/user/repos", false),
        op("getRepo", "Récupérer dépôt", "GET", "/repos/{id}", false),
        op("createIssue", "Créer issue", "POST", "/repos/{id}/issues", true),
        op("listIssues", "Lister issues", "GET", "/repos/{id}/issues", false),
        op("createPr", "Créer pull request", "POST", "/repos/{id}/pulls", true),
        op("listPrs", "Lister pull requests", "GET", "/repos/{id}/pulls", false),
        op("getContent", "Lire contenu", "GET", "/repos/{id}/contents", false),
        op("listCommits", "Lister commits", "GET", "/repos/{id}/commits", false),
        op("listBranches", "Lister branches", "GET", "/repos/{id}/branches", false),
        op("createRelease", "Créer release", "POST", "/repos/{id}/releases", true),
        op("dispatch", "Déclencher un workflow", "POST", "/repos/{id}/dispatches", true),
    ]));
    v.push(si("svc.gitlab", "GitLab", "Gitlab", "#fc6d26", "/api/v4", "gitlabApi,httpBearerAuth", Bearer, vec![
        op("projects", "Mes projets", "GET", "/projects?membership=true", false),
        op("createIssue", "Créer issue", "POST", "/projects/{id}/issues", true),
        op("listIssues", "Lister issues", "GET", "/projects/{id}/issues", false),
        op("mergeRequests", "Lister merge requests", "GET", "/projects/{id}/merge_requests", false),
        op("createMr", "Créer merge request", "POST", "/projects/{id}/merge_requests", true),
        op("pipelines", "Pipelines", "GET", "/projects/{id}/pipelines", false),
        op("triggerPipeline", "Déclencher un pipeline", "POST", "/projects/{id}/pipeline", true),
    ]));
    v.push(s("svc.bitbucket", "Bitbucket", "GitBranch", "#0052cc", "https://api.bitbucket.org/2.0", "bitbucketApi", Basic, vec![
        op("repos", "Dépôts", "GET", "/repositories/{id}", false),
    ]));
    v.push(si("svc.jira", "Jira", "SquareKanban", "#0052cc", "/rest/api/3", "jiraApi", Basic, vec![
        op("search", "Rechercher (JQL)", "GET", "/search", false),
        op("getIssue", "Récupérer ticket", "GET", "/issue/{id}", false),
        op("createIssue", "Créer ticket", "POST", "/issue", true),
        op("updateIssue", "Mettre à jour ticket", "PUT", "/issue/{id}", true),
        op("addComment", "Ajouter commentaire", "POST", "/issue/{id}/comment", true),
        op("transitions", "Transitions disponibles", "GET", "/issue/{id}/transitions", false),
        op("doTransition", "Changer le statut", "POST", "/issue/{id}/transitions", true),
        op("projects", "Lister projets", "GET", "/project", false),
    ]));
    v.push(si("svc.confluence", "Confluence", "FileText", "#172b4d", "/wiki/rest/api", "confluenceApi", Basic, vec![
        op("content", "Contenu", "GET", "/content", false),
        op("createPage", "Créer page", "POST", "/content", true),
    ]));
    v.push(s("svc.linear", "Linear", "SquareKanban", "#5e6ad2", "https://api.linear.app/graphql", "linearApi", Header("Authorization"), vec![
        op("graphql", "Requête GraphQL", "POST", "", true),
    ]));
    v.push(s("svc.sentry", "Sentry", "Bug", "#362d59", "https://sentry.io/api/0", "sentryApi,httpBearerAuth", Bearer, vec![
        op("projects", "Projets", "GET", "/projects/", false),
        op("issues", "Issues", "GET", "/projects/{id}/issues/", false),
    ]));
    v.push(s("svc.pagerduty", "PagerDuty", "Siren", "#06ac38", "https://api.pagerduty.com", "pagerDutyApi", Header("Authorization"), vec![
        op("incidents", "Incidents", "GET", "/incidents", false),
        op("createIncident", "Créer incident", "POST", "/incidents", true),
    ]));
    v.push(s("svc.opsgenie", "Opsgenie", "Siren", "#172b4d", "https://api.opsgenie.com/v2", "opsgenieApi", Header("Authorization"), vec![
        op("alerts", "Alertes", "GET", "/alerts", false),
        op("createAlert", "Créer alerte", "POST", "/alerts", true),
    ]));
    v.push(s("svc.circleci", "CircleCI", "Container", "#343434", "https://circleci.com/api/v2", "apiKeyAuth", Header("Circle-Token"), vec![
        op("pipelines", "Pipelines", "GET", "/project/{id}/pipeline", false),
    ]));
    v.push(s("svc.vercel", "Vercel", "Triangle", "#000000", "https://api.vercel.com", "vercelApi,httpBearerAuth", Bearer, vec![
        op("projects", "Projets", "GET", "/v9/projects", false),
        op("deployments", "Déploiements", "GET", "/v6/deployments", false),
    ]));
    v.push(s("svc.netlify", "Netlify", "Cloud", "#00c7b7", "https://api.netlify.com/api/v1", "netlifyApi,httpBearerAuth", Bearer, vec![
        op("sites", "Sites", "GET", "/sites", false),
    ]));
    v.push(s("svc.cloudflare", "Cloudflare", "Cloud", "#f38020", "https://api.cloudflare.com/client/v4", "cloudflareApi,httpBearerAuth", Bearer, vec![
        op("zones", "Zones", "GET", "/zones", false),
        op("dnsRecords", "Enregistrements DNS", "GET", "/zones/{id}/dns_records", false),
    ]));
    v.push(s("svc.digitalocean", "DigitalOcean", "Cloud", "#0080ff", "https://api.digitalocean.com/v2", "digitalOceanApi,httpBearerAuth", Bearer, vec![
        op("droplets", "Droplets", "GET", "/droplets", false),
    ]));
    v.push(s("svc.npm", "npm", "Package", "#cb3837", "https://registry.npmjs.org", "npmApi", None, vec![
        op("package", "Infos paquet", "GET", "/{id}", false),
    ]));
    v.push(s("svc.grafana", "Grafana", "ChartArea", "#f46800", "https://grafana.example/api", "grafanaApi,httpBearerAuth", Bearer, vec![
        op("dashboards", "Dashboards", "GET", "/search?type=dash-db", false),
    ]));
    v.push(s("svc.datadog", "Datadog", "ChartArea", "#632ca6", "https://api.datadoghq.com/api/v1", "datadogApi", Header("DD-API-KEY"), vec![
        op("events", "Événements", "GET", "/events", false),
        op("postEvent", "Publier événement", "POST", "/events", true),
    ]));

    // ── Productivité / gestion de projet ──
    v.push(s("svc.notion", "Notion", "FileText", "#000000", "https://api.notion.com/v1", "notionApi,httpBearerAuth", Bearer, vec![
        op("search", "Rechercher", "POST", "/search", true),
        op("getPage", "Récupérer page", "GET", "/pages/{id}", false),
        op("createPage", "Créer page", "POST", "/pages", true),
        op("queryDb", "Interroger base", "POST", "/databases/{id}/query", true),
        op("updatePage", "Mettre à jour page", "PATCH", "/pages/{id}", true),
        op("getDatabase", "Lire base", "GET", "/databases/{id}", false),
        op("getBlocks", "Blocs enfants", "GET", "/blocks/{id}/children", false),
        op("appendBlocks", "Ajouter des blocs", "PATCH", "/blocks/{id}/children", true),
        op("listUsers", "Lister utilisateurs", "GET", "/users", false),
    ]));
    v.push(s("svc.airtable", "Airtable", "Table", "#18bfff", "https://api.airtable.com/v0", "airtableApi,httpBearerAuth", Bearer, vec![
        op("list", "Lister enregistrements", "GET", "/{id}", false),
        op("create", "Créer", "POST", "/{id}", true),
        op("update", "Mettre à jour", "PATCH", "/{id}", true),
        op("delete", "Supprimer", "DELETE", "/{id}", false),
    ]));
    v.push(s("svc.coda", "Coda", "FileText", "#f46a54", "https://coda.io/apis/v1", "codaApi,httpBearerAuth", Bearer, vec![
        op("docs", "Documents", "GET", "/docs", false),
        op("rows", "Lignes d'une table", "GET", "/docs/{id}/tables", false),
    ]));
    v.push(s("svc.clickup", "ClickUp", "SquareKanban", "#7b68ee", "https://api.clickup.com/api/v2", "clickUpApi", Header("Authorization"), vec![
        op("tasks", "Tâches d'une liste", "GET", "/list/{id}/task", false),
        op("createTask", "Créer tâche", "POST", "/list/{id}/task", true),
    ]));
    v.push(s("svc.asana", "Asana", "SquareKanban", "#f06a6a", "https://app.asana.com/api/1.0", "asanaApi,httpBearerAuth", Bearer, vec![
        op("tasks", "Tâches", "GET", "/tasks", false),
        op("createTask", "Créer tâche", "POST", "/tasks", true),
        op("getTask", "Lire tâche", "GET", "/tasks/{id}", false),
        op("updateTask", "Mettre à jour tâche", "PUT", "/tasks/{id}", true),
        op("addComment", "Ajouter commentaire", "POST", "/tasks/{id}/stories", true),
        op("projects", "Projets", "GET", "/projects", false),
    ]));
    v.push(s("svc.trello", "Trello", "SquareKanban", "#0079bf", "https://api.trello.com/1", "trelloApi", Query("key"), vec![
        op("boards", "Mes tableaux", "GET", "/members/me/boards", false),
        op("createCard", "Créer carte", "POST", "/cards", true),
        op("getBoard", "Lire tableau", "GET", "/boards/{id}", false),
        op("lists", "Listes du tableau", "GET", "/boards/{id}/lists", false),
        op("cards", "Cartes d'une liste", "GET", "/lists/{id}/cards", false),
        op("updateCard", "Mettre à jour carte", "PUT", "/cards/{id}", true),
        op("deleteCard", "Supprimer carte", "DELETE", "/cards/{id}", false),
    ]));
    v.push(s("svc.monday", "monday.com", "SquareKanban", "#ff3d57", "https://api.monday.com/v2", "mondayComApi", Header("Authorization"), vec![
        op("graphql", "Requête GraphQL", "POST", "", true),
    ]));
    v.push(s("svc.todoist", "Todoist", "ListChecks", "#e44332", "https://api.todoist.com/rest/v2", "todoistApi,httpBearerAuth", Bearer, vec![
        op("tasks", "Tâches", "GET", "/tasks", false),
        op("createTask", "Créer tâche", "POST", "/tasks", true),
        op("close", "Terminer tâche", "POST", "/tasks/{id}/close", false),
        op("updateTask", "Mettre à jour tâche", "POST", "/tasks/{id}", true),
        op("reopen", "Rouvrir tâche", "POST", "/tasks/{id}/reopen", false),
        op("deleteTask", "Supprimer tâche", "DELETE", "/tasks/{id}", false),
        op("projects", "Projets", "GET", "/projects", false),
    ]));
    v.push(s("svc.height", "Height", "SquareKanban", "#2d2b38", "https://api.height.app", "httpBearerAuth", Bearer, vec![
        op("tasks", "Tâches", "GET", "/tasks", false),
    ]));

    // ── CRM / support ──
    v.push(s("svc.hubspot", "HubSpot", "Contact", "#ff7a59", "https://api.hubapi.com", "hubspotApi,httpBearerAuth", Bearer, vec![
        op("contacts", "Contacts", "GET", "/crm/v3/objects/contacts", false),
        op("createContact", "Créer contact", "POST", "/crm/v3/objects/contacts", true),
        op("deals", "Affaires", "GET", "/crm/v3/objects/deals", false),
        op("getContact", "Lire contact", "GET", "/crm/v3/objects/contacts/{id}", false),
        op("updateContact", "Mettre à jour contact", "PATCH", "/crm/v3/objects/contacts/{id}", true),
        op("companies", "Entreprises", "GET", "/crm/v3/objects/companies", false),
        op("createDeal", "Créer affaire", "POST", "/crm/v3/objects/deals", true),
        op("searchContacts", "Rechercher contacts", "POST", "/crm/v3/objects/contacts/search", true),
    ]));
    v.push(s("svc.salesforce", "Salesforce", "Cloud", "#00a1e0", "https://example.salesforce.com/services/data/v60.0", "salesforceApi,httpBearerAuth", Bearer, vec![
        op("query", "Requête SOQL", "GET", "/query", false),
        op("create", "Créer objet", "POST", "/sobjects/{id}", true),
    ]));
    v.push(s("svc.pipedrive", "Pipedrive", "Contact", "#017737", "https://api.pipedrive.com/v1", "pipedriveApi", Query("api_token"), vec![
        op("deals", "Affaires", "GET", "/deals", false),
        op("createDeal", "Créer affaire", "POST", "/deals", true),
        op("persons", "Personnes", "GET", "/persons", false),
    ]));
    v.push(si("svc.zendesk", "Zendesk", "MessageSquare", "#03363d", "/api/v2", "zendeskApi", Basic, vec![
        op("tickets", "Tickets", "GET", "/tickets.json", false),
        op("createTicket", "Créer ticket", "POST", "/tickets.json", true),
    ]));
    v.push(s("svc.freshdesk", "Freshdesk", "MessageSquare", "#25c16f", "https://example.freshdesk.com/api/v2", "freshdeskApi", Basic, vec![
        op("tickets", "Tickets", "GET", "/tickets", false),
        op("createTicket", "Créer ticket", "POST", "/tickets", true),
    ]));
    v.push(s("svc.intercom", "Intercom", "MessageSquare", "#1f8ded", "https://api.intercom.io", "intercomApi,httpBearerAuth", Bearer, vec![
        op("contacts", "Contacts", "GET", "/contacts", false),
        op("createMessage", "Envoyer message", "POST", "/messages", true),
    ]));
    v.push(s("svc.front", "Front", "MessageSquare", "#a857ff", "https://api2.frontapp.com", "httpBearerAuth", Bearer, vec![
        op("conversations", "Conversations", "GET", "/conversations", false),
    ]));
    v.push(s("svc.helpscout", "Help Scout", "MessageSquare", "#1292ee", "https://api.helpscout.net/v2", "httpBearerAuth", Bearer, vec![
        op("conversations", "Conversations", "GET", "/conversations", false),
    ]));

    // ── Paiements / e-commerce ──
    v.push(s("svc.stripe", "Stripe", "CreditCard", "#635bff", "https://api.stripe.com/v1", "stripeApi", Bearer, vec![
        op("customers", "Clients", "GET", "/customers", false),
        op("createCustomer", "Créer client", "POST", "/customers", true),
        op("charges", "Paiements", "GET", "/charges", false),
        op("createPaymentIntent", "Créer PaymentIntent", "POST", "/payment_intents", true),
        op("getCustomer", "Lire client", "GET", "/customers/{id}", false),
        op("updateCustomer", "Mettre à jour client", "POST", "/customers/{id}", true),
        op("refund", "Rembourser", "POST", "/refunds", true),
        op("listInvoices", "Lister factures", "GET", "/invoices", false),
        op("createInvoice", "Créer facture", "POST", "/invoices", true),
        op("subscriptions", "Abonnements", "GET", "/subscriptions", false),
        op("createSubscription", "Créer abonnement", "POST", "/subscriptions", true),
        op("products", "Produits", "GET", "/products", false),
        op("checkoutSession", "Session de paiement", "POST", "/checkout/sessions", true),
        op("events", "Événements", "GET", "/events", false),
    ]));
    v.push(s("svc.paypal", "PayPal", "CreditCard", "#003087", "https://api-m.paypal.com/v2", "paypalApi,httpBearerAuth", Bearer, vec![
        op("orders", "Récupérer commande", "GET", "/checkout/orders/{id}", false),
        op("createOrder", "Créer commande", "POST", "/checkout/orders", true),
    ]));
    v.push(si("svc.shopify", "Shopify", "ShoppingCart", "#96bf48", "/admin/api/2024-04", "shopifyApi", Header("X-Shopify-Access-Token"), vec![
        op("products", "Produits", "GET", "/products.json", false),
        op("orders", "Commandes", "GET", "/orders.json", false),
        op("createProduct", "Créer produit", "POST", "/products.json", true),
        op("getProduct", "Lire produit", "GET", "/products/{id}.json", false),
        op("updateProduct", "Mettre à jour produit", "PUT", "/products/{id}.json", true),
        op("getOrder", "Lire commande", "GET", "/orders/{id}.json", false),
        op("customers", "Clients", "GET", "/customers.json", false),
        op("inventory", "Niveaux de stock", "GET", "/inventory_levels.json", false),
    ]));
    v.push(si("svc.woocommerce", "WooCommerce", "ShoppingCart", "#96588a", "/wp-json/wc/v3", "wooCommerceApi", Basic, vec![
        op("products", "Produits", "GET", "/products", false),
        op("orders", "Commandes", "GET", "/orders", false),
    ]));
    v.push(s("svc.lemonsqueezy", "Lemon Squeezy", "CreditCard", "#ffc233", "https://api.lemonsqueezy.com/v1", "lemonSqueezyApi,httpBearerAuth", Bearer, vec![
        op("orders", "Commandes", "GET", "/orders", false),
        op("subscriptions", "Abonnements", "GET", "/subscriptions", false),
    ]));
    v.push(s("svc.paddle", "Paddle", "CreditCard", "#ffdd00", "https://api.paddle.com", "paddleApi,httpBearerAuth", Bearer, vec![
        op("transactions", "Transactions", "GET", "/transactions", false),
    ]));
    v.push(s("svc.gumroad", "Gumroad", "ShoppingCart", "#ff90e8", "https://api.gumroad.com/v2", "apiKeyAuth", Query("access_token"), vec![
        op("sales", "Ventes", "GET", "/sales", false),
        op("products", "Produits", "GET", "/products", false),
    ]));

    // ── Réseaux sociaux ──
    v.push(s("svc.twitter", "X (Twitter)", "Twitter", "#000000", "https://api.twitter.com/2", "twitterOAuth2Api,httpBearerAuth", Bearer, vec![
        op("me", "Mon compte", "GET", "/users/me", false),
        op("tweet", "Publier", "POST", "/tweets", true),
    ]));
    v.push(s("svc.linkedin", "LinkedIn", "Linkedin", "#0a66c2", "https://api.linkedin.com/v2", "linkedInOAuth2Api,httpBearerAuth", Bearer, vec![
        op("me", "Mon profil", "GET", "/me", false),
        op("share", "Publier", "POST", "/ugcPosts", true),
    ]));
    v.push(s("svc.facebook", "Facebook", "Facebook", "#1877f2", "https://graph.facebook.com/v20.0", "facebookGraphApi,httpBearerAuth", Bearer, vec![
        op("me", "Mon profil", "GET", "/me", false),
        op("publish", "Publier (page)", "POST", "/{id}/feed", true),
    ]));
    v.push(s("svc.mastodon", "Mastodon", "Globe", "#6364ff", "https://mastodon.social/api/v1", "mastodonApi,httpBearerAuth", Bearer, vec![
        op("statuses", "Publier", "POST", "/statuses", true),
        op("timeline", "Fil public", "GET", "/timelines/public", false),
    ]));
    v.push(s("svc.reddit", "Reddit", "Globe", "#ff4500", "https://oauth.reddit.com", "httpBearerAuth", Bearer, vec![
        op("me", "Mon compte", "GET", "/api/v1/me", false),
    ]));
    v.push(s("svc.twitch", "Twitch", "Video", "#9146ff", "https://api.twitch.tv/helix", "httpBearerAuth", Bearer, vec![
        op("users", "Utilisateurs", "GET", "/users", false),
        op("streams", "Streams", "GET", "/streams", false),
    ]));

    // ── IA / LLM ──
    v.push(s("svc.openai", "OpenAI", "Sparkles", "#10a37f", "https://api.openai.com/v1", "openAiApi,httpBearerAuth", Bearer, vec![
        op("chat", "Chat completions", "POST", "/chat/completions", true),
        op("embeddings", "Embeddings", "POST", "/embeddings", true),
        op("images", "Génération d'image", "POST", "/images/generations", true),
        op("models", "Modèles", "GET", "/models", false),
        op("transcribe", "Transcription audio", "POST", "/audio/transcriptions", true),
        op("speech", "Synthèse vocale", "POST", "/audio/speech", true),
        op("moderations", "Modération", "POST", "/moderations", true),
        op("listFiles", "Lister fichiers", "GET", "/files", false),
    ]));
    v.push(s("svc.anthropic", "Anthropic", "Sparkles", "#d4a373", "https://api.anthropic.com/v1", "anthropicApi", Header("x-api-key"), vec![
        op("messages", "Messages", "POST", "/messages", true),
        op("countTokens", "Compter les tokens", "POST", "/messages/count_tokens", true),
        op("models", "Modèles", "GET", "/models", false),
    ]));
    v.push(s("svc.mistral", "Mistral AI", "Sparkles", "#ff7000", "https://api.mistral.ai/v1", "mistralApi,httpBearerAuth", Bearer, vec![
        op("chat", "Chat completions", "POST", "/chat/completions", true),
        op("embeddings", "Embeddings", "POST", "/embeddings", true),
    ]));
    v.push(s("svc.cohere", "Cohere", "Sparkles", "#39594c", "https://api.cohere.com/v2", "cohereApi,httpBearerAuth", Bearer, vec![
        op("chat", "Chat", "POST", "/chat", true),
        op("embed", "Embeddings", "POST", "/embed", true),
    ]));
    v.push(s("svc.huggingface", "Hugging Face", "Sparkles", "#ff9d00", "https://api-inference.huggingface.co/models", "huggingFaceApi,httpBearerAuth", Bearer, vec![
        op("infer", "Inférence (modèle {id})", "POST", "/{id}", true),
    ]));
    v.push(s("svc.replicate", "Replicate", "Sparkles", "#000000", "https://api.replicate.com/v1", "replicateApi", Header("Authorization"), vec![
        op("predictions", "Créer prédiction", "POST", "/predictions", true),
        op("getPrediction", "Récupérer prédiction", "GET", "/predictions/{id}", false),
    ]));
    v.push(s("svc.elevenlabs", "ElevenLabs", "Sparkles", "#000000", "https://api.elevenlabs.io/v1", "elevenLabsApi", Header("xi-api-key"), vec![
        op("tts", "Synthèse vocale", "POST", "/text-to-speech/{id}", true),
        op("voices", "Voix", "GET", "/voices", false),
    ]));
    v.push(s("svc.stability", "Stability AI", "Sparkles", "#000000", "https://api.stability.ai/v2beta", "stabilityAiApi,httpBearerAuth", Bearer, vec![
        op("generate", "Générer image", "POST", "/stable-image/generate/core", true),
    ]));
    v.push(s("svc.perplexity", "Perplexity", "Sparkles", "#20808d", "https://api.perplexity.ai", "perplexityApi,httpBearerAuth", Bearer, vec![
        op("chat", "Chat completions", "POST", "/chat/completions", true),
    ]));
    v.push(s("svc.groq", "Groq", "Sparkles", "#f55036", "https://api.groq.com/openai/v1", "groqApi,httpBearerAuth", Bearer, vec![
        op("chat", "Chat completions", "POST", "/chat/completions", true),
    ]));
    v.push(s("svc.deepl", "DeepL", "Sparkles", "#0f2b46", "https://api-free.deepl.com/v2", "deepLApi", Header("Authorization"), vec![
        op("translate", "Traduire", "POST", "/translate", true),
    ]));

    // ── Données / recherche vectorielle ──
    v.push(s("svc.pinecone", "Pinecone", "Boxes", "#000000", "https://example.pinecone.io", "pineconeApi", Header("Api-Key"), vec![
        op("query", "Requête", "POST", "/query", true),
        op("upsert", "Upsert vecteurs", "POST", "/vectors/upsert", true),
    ]));
    v.push(si("svc.weaviate", "Weaviate", "Boxes", "#fd4239", "/v1", "weaviateApi,httpBearerAuth", Bearer, vec![
        op("objects", "Objets", "GET", "/objects", false),
        op("graphql", "GraphQL", "POST", "/graphql", true),
    ]));
    v.push(si("svc.qdrant", "Qdrant", "Boxes", "#dc244c", "", "qdrantApi", Header("api-key"), vec![
        op("collections", "Collections", "GET", "/collections", false),
        op("search", "Rechercher", "POST", "/collections/{id}/points/search", true),
    ]));
    v.push(s("svc.algolia", "Algolia", "Search", "#003dff", "https://example.algolia.net/1", "algoliaApi", Header("X-Algolia-API-Key"), vec![
        op("search", "Rechercher", "POST", "/indexes/{id}/query", true),
    ]));
    v.push(si("svc.meilisearch", "Meilisearch", "Search", "#ff5caa", "", "meilisearchApi,httpBearerAuth", Bearer, vec![
        op("search", "Rechercher", "POST", "/indexes/{id}/search", true),
        op("documents", "Ajouter documents", "POST", "/indexes/{id}/documents", true),
    ]));
    v.push(si("svc.typesense", "Typesense", "Search", "#1a1a2e", "", "typesenseApi", Header("X-TYPESENSE-API-KEY"), vec![
        op("search", "Rechercher", "GET", "/collections/{id}/documents/search", false),
    ]));

    // ── CMS / contenu ──
    v.push(si("svc.wordpress", "WordPress", "FileText", "#21759b", "/wp-json/wp/v2", "wordpressApi", Basic, vec![
        op("posts", "Articles", "GET", "/posts", false),
        op("createPost", "Créer article", "POST", "/posts", true),
        op("pages", "Pages", "GET", "/pages", false),
    ]));
    v.push(si("svc.ghost", "Ghost", "FileText", "#15171a", "/ghost/api/admin", "ghostApi", Header("Authorization"), vec![
        op("posts", "Articles", "GET", "/posts/", false),
        op("createPost", "Créer article", "POST", "/posts/", true),
    ]));
    v.push(s("svc.webflow", "Webflow", "Globe", "#146ef5", "https://api.webflow.com/v2", "webflowApi,httpBearerAuth", Bearer, vec![
        op("sites", "Sites", "GET", "/sites", false),
        op("items", "Éléments d'une collection", "GET", "/collections/{id}/items", false),
    ]));
    v.push(s("svc.contentful", "Contentful", "FileText", "#2478cc", "https://cdn.contentful.com/spaces", "contentfulApi,httpBearerAuth", Bearer, vec![
        op("entries", "Entrées", "GET", "/{id}/entries", false),
    ]));
    v.push(si("svc.strapi", "Strapi", "FileText", "#4945ff", "/api", "strapiApi,httpBearerAuth", Bearer, crud("/{id}")));
    v.push(s("svc.sanity", "Sanity", "FileText", "#f03e2f", "https://example.api.sanity.io/v2021-10-21", "httpBearerAuth", Bearer, vec![
        op("query", "Requête GROQ", "GET", "/data/query/production", false),
    ]));
    v.push(s("svc.discourse", "Discourse", "MessageSquare", "#000000", "https://meta.discourse.org", "apiKeyAuth", Header("Api-Key"), vec![
        op("latest", "Sujets récents", "GET", "/latest.json", false),
        op("createPost", "Publier", "POST", "/posts.json", true),
    ]));

    // ── Divers / utilitaires ──
    v.push(s("svc.openweather", "OpenWeatherMap", "CloudSun", "#eb6e4b", "https://api.openweathermap.org/data/2.5", "openWeatherMapApi", Query("appid"), vec![
        op("current", "Météo actuelle", "GET", "/weather", false),
        op("forecast", "Prévisions", "GET", "/forecast", false),
    ]));
    v.push(s("svc.mapbox", "Mapbox", "Map", "#000000", "https://api.mapbox.com", "mapboxApi", Query("access_token"), vec![
        op("geocode", "Géocoder", "GET", "/geocoding/v5/mapbox.places/{id}.json", false),
    ]));
    v.push(s("svc.newsapi", "News API", "Newspaper", "#c4302b", "https://newsapi.org/v2", "newsApi", Query("apiKey"), vec![
        op("topHeadlines", "À la une", "GET", "/top-headlines", false),
        op("everything", "Recherche", "GET", "/everything", false),
    ]));
    v.push(s("svc.nasa", "NASA", "Rocket", "#0b3d91", "https://api.nasa.gov", "nasaApi", Query("api_key"), vec![
        op("apod", "Image du jour", "GET", "/planetary/apod", false),
    ]));
    v.push(s("svc.calendly", "Calendly", "Calendar", "#006bff", "https://api.calendly.com", "calendlyApi,httpBearerAuth", Bearer, vec![
        op("me", "Mon compte", "GET", "/users/me", false),
        op("events", "Événements", "GET", "/scheduled_events", false),
    ]));
    v.push(s("svc.zoom", "Zoom", "Video", "#2d8cff", "https://api.zoom.us/v2", "httpBearerAuth", Bearer, vec![
        op("meetings", "Réunions", "GET", "/users/me/meetings", false),
        op("createMeeting", "Créer réunion", "POST", "/users/me/meetings", true),
    ]));
    v.push(s("svc.bitly", "Bitly", "Globe", "#ee6123", "https://api-ssl.bitly.com/v4", "httpBearerAuth", Bearer, vec![
        op("shorten", "Raccourcir", "POST", "/shorten", true),
    ]));
    v.push(si("svc.supabase", "Supabase (REST)", "Database", "#3ecf8e", "/rest/v1", "supabase", Header("apikey"), vec![
        op("select", "Lire table", "GET", "/{id}", false),
        op("insert", "Insérer", "POST", "/{id}", true),
    ]));
    v.push(s("svc.firebase", "Firebase (Firestore)", "Database", "#ffca28", "https://firestore.googleapis.com/v1", "googleAccessToken,httpBearerAuth", Bearer, vec![
        op("getDoc", "Lire document", "GET", "/{id}", false),
    ]));
    v.push(s("svc.airtableMeta", "Airtable (méta)", "Table", "#18bfff", "https://api.airtable.com/v0/meta", "airtableApi,httpBearerAuth", Bearer, vec![
        op("bases", "Lister bases", "GET", "/bases", false),
    ]));

    v
}

/// Petite fuite mémoire volontaire pour obtenir un `&'static str` à partir d'une
/// `String` calculée (chemins CRUD). Le catalogue est construit une seule fois.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}
