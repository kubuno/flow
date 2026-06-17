//! Safe expression engine for `{{ … }}` placeholders.
//!
//! Unlike a raw JS eval, this is a small, sandboxed language: it can only read
//! from the provided JSON context and call a fixed set of built-in functions.
//! No I/O, no host access, no unbounded recursion (depth-limited parser).
//!
//! Grammar (precedence low → high):
//!   ternary  `c ? a : b`
//!   or       `||`
//!   and      `&&`
//!   equality `== != ?` (`?` not used; `!=`/`==`)
//!   compare  `< > <= >=`
//!   additive `+ -`        (`+` = numeric add OR string concat)
//!   multipl. `* / %`
//!   unary    `- !`
//!   postfix  `.member  [index]  (call)`
//!   primary  literal | identifier | `(expr)` | `[array]`
//!
//! Identifiers may contain `$` (so `$json`, `$now`, `$node` work). An unknown
//! identifier or a missing member resolves to `null` (never an error), matching
//! the forgiving behaviour users expect from n8n / Make expressions.

use chrono::{Datelike, Duration, Timelike, Utc};
use serde_json::{json, Map, Value};

/// Evaluate a single expression source against a JSON context.
/// Returns `Value::Null` on any parse/eval failure (forgiving by design).
pub fn evaluate(src: &str, ctx: &Value) -> Value {
    let tokens = match lex(src) {
        Ok(t) => t,
        Err(_) => return Value::Null,
    };
    let mut p = Parser { tokens, pos: 0, depth: 0 };
    let ast = match p.parse_expr() {
        Ok(a) => a,
        Err(_) => return Value::Null,
    };
    if p.pos != p.tokens.len() {
        // Trailing garbage → treat as failure (null).
        return Value::Null;
    }
    eval_node(&ast, ctx)
}

// ── Tokenizer ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    True,
    False,
    Null,
    // Punctuation / operators
    Plus, Minus, Star, Slash, Percent,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or, Not,
    Question, Colon,
    LParen, RParen, LBracket, RBracket, Comma, Dot,
}

fn lex(src: &str) -> Result<Vec<Tok>, ()> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() { i += 1; continue; }
        match c {
            '+' => { out.push(Tok::Plus); i += 1; }
            '-' => { out.push(Tok::Minus); i += 1; }
            '*' => { out.push(Tok::Star); i += 1; }
            '/' => { out.push(Tok::Slash); i += 1; }
            '%' => { out.push(Tok::Percent); i += 1; }
            '(' => { out.push(Tok::LParen); i += 1; }
            ')' => { out.push(Tok::RParen); i += 1; }
            '[' => { out.push(Tok::LBracket); i += 1; }
            ']' => { out.push(Tok::RBracket); i += 1; }
            ',' => { out.push(Tok::Comma); i += 1; }
            '.' => { out.push(Tok::Dot); i += 1; }
            '?' => { out.push(Tok::Question); i += 1; }
            ':' => { out.push(Tok::Colon); i += 1; }
            '=' if i + 1 < b.len() && b[i + 1] == '=' => { out.push(Tok::Eq); i += 2; }
            '=' => return Err(()), // bare `=` not allowed
            '!' => {
                if i + 1 < b.len() && b[i + 1] == '=' { out.push(Tok::Ne); i += 2; }
                else { out.push(Tok::Not); i += 1; }
            }
            '<' => {
                if i + 1 < b.len() && b[i + 1] == '=' { out.push(Tok::Le); i += 2; }
                else { out.push(Tok::Lt); i += 1; }
            }
            '>' => {
                if i + 1 < b.len() && b[i + 1] == '=' { out.push(Tok::Ge); i += 2; }
                else { out.push(Tok::Gt); i += 1; }
            }
            '&' if i + 1 < b.len() && b[i + 1] == '&' => { out.push(Tok::And); i += 2; }
            '&' => return Err(()),
            '|' if i + 1 < b.len() && b[i + 1] == '|' => { out.push(Tok::Or); i += 2; }
            '|' => return Err(()),
            '\'' | '"' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                while i < b.len() && b[i] != quote {
                    if b[i] == '\\' && i + 1 < b.len() {
                        i += 1;
                        s.push(match b[i] {
                            'n' => '\n', 't' => '\t', 'r' => '\r',
                            other => other,
                        });
                    } else {
                        s.push(b[i]);
                    }
                    i += 1;
                }
                if i >= b.len() { return Err(()); } // unterminated string
                i += 1; // closing quote
                out.push(Tok::Str(s));
            }
            _ if c.is_ascii_digit() => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.') { i += 1; }
                let num: String = b[start..i].iter().collect();
                out.push(Tok::Num(num.parse().map_err(|_| ())?));
            }
            _ if c.is_alphabetic() || c == '_' || c == '$' => {
                let start = i;
                while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_' || b[i] == '$') { i += 1; }
                let word: String = b[start..i].iter().collect();
                out.push(match word.as_str() {
                    "true"  => Tok::True,
                    "false" => Tok::False,
                    "null"  => Tok::Null,
                    "and"   => Tok::And,
                    "or"    => Tok::Or,
                    _       => Tok::Ident(word),
                });
            }
            _ => return Err(()),
        }
    }
    Ok(out)
}

// ── AST ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Node {
    Lit(Value),
    Var(String),
    Array(Vec<Node>),
    Unary(Tok, Box<Node>),
    Binary(Tok, Box<Node>, Box<Node>),
    Ternary(Box<Node>, Box<Node>, Box<Node>),
    Member(Box<Node>, String),
    Index(Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

struct Parser {
    tokens: Vec<Tok>,
    pos:    usize,
    depth:  usize,
}

const MAX_DEPTH: usize = 64;

impl Parser {
    fn peek(&self) -> Option<&Tok> { self.tokens.get(self.pos) }
    fn next(&mut self) -> Option<Tok> { let t = self.tokens.get(self.pos).cloned(); self.pos += 1; t }
    fn eat(&mut self, t: &Tok) -> Result<(), ()> {
        if self.peek() == Some(t) { self.pos += 1; Ok(()) } else { Err(()) }
    }

    fn parse_expr(&mut self) -> Result<Node, ()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { return Err(()); }
        let r = self.parse_ternary();
        self.depth -= 1;
        r
    }

    fn parse_ternary(&mut self) -> Result<Node, ()> {
        let cond = self.parse_or()?;
        if self.peek() == Some(&Tok::Question) {
            self.pos += 1;
            let a = self.parse_expr()?;
            self.eat(&Tok::Colon)?;
            let b = self.parse_expr()?;
            return Ok(Node::Ternary(Box::new(cond), Box::new(a), Box::new(b)));
        }
        Ok(cond)
    }

    fn parse_or(&mut self) -> Result<Node, ()> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Tok::Or) {
            self.pos += 1;
            let right = self.parse_and()?;
            left = Node::Binary(Tok::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Node, ()> {
        let mut left = self.parse_equality()?;
        while self.peek() == Some(&Tok::And) {
            self.pos += 1;
            let right = self.parse_equality()?;
            left = Node::Binary(Tok::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Node, ()> {
        let mut left = self.parse_compare()?;
        while matches!(self.peek(), Some(Tok::Eq) | Some(Tok::Ne)) {
            let op = self.next().unwrap();
            let right = self.parse_compare()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_compare(&mut self) -> Result<Node, ()> {
        let mut left = self.parse_add()?;
        while matches!(self.peek(), Some(Tok::Lt) | Some(Tok::Gt) | Some(Tok::Le) | Some(Tok::Ge)) {
            let op = self.next().unwrap();
            let right = self.parse_add()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Node, ()> {
        let mut left = self.parse_mul()?;
        while matches!(self.peek(), Some(Tok::Plus) | Some(Tok::Minus)) {
            let op = self.next().unwrap();
            let right = self.parse_mul()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Node, ()> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Some(Tok::Star) | Some(Tok::Slash) | Some(Tok::Percent)) {
            let op = self.next().unwrap();
            let right = self.parse_unary()?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Node, ()> {
        if matches!(self.peek(), Some(Tok::Minus) | Some(Tok::Not)) {
            let op = self.next().unwrap();
            let operand = self.parse_unary()?;
            return Ok(Node::Unary(op, Box::new(operand)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Node, ()> {
        let mut node = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Tok::Dot) => {
                    self.pos += 1;
                    match self.next() {
                        Some(Tok::Ident(name)) => node = Node::Member(Box::new(node), name),
                        // allow keyword-like members? keep strict.
                        _ => return Err(()),
                    }
                }
                Some(Tok::LBracket) => {
                    self.pos += 1;
                    let idx = self.parse_expr()?;
                    self.eat(&Tok::RBracket)?;
                    node = Node::Index(Box::new(node), Box::new(idx));
                }
                _ => break,
            }
        }
        Ok(node)
    }

    fn parse_primary(&mut self) -> Result<Node, ()> {
        match self.next() {
            Some(Tok::Num(n))   => Ok(Node::Lit(json!(n))),
            Some(Tok::Str(s))   => Ok(Node::Lit(Value::String(s))),
            Some(Tok::True)     => Ok(Node::Lit(Value::Bool(true))),
            Some(Tok::False)    => Ok(Node::Lit(Value::Bool(false))),
            Some(Tok::Null)     => Ok(Node::Lit(Value::Null)),
            Some(Tok::Ident(name)) => {
                // Function call?
                if self.peek() == Some(&Tok::LParen) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    if self.peek() != Some(&Tok::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.peek() == Some(&Tok::Comma) { self.pos += 1; continue; }
                            break;
                        }
                    }
                    self.eat(&Tok::RParen)?;
                    Ok(Node::Call(name, args))
                } else {
                    Ok(Node::Var(name))
                }
            }
            Some(Tok::LParen) => {
                let e = self.parse_expr()?;
                self.eat(&Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::LBracket) => {
                let mut items = Vec::new();
                if self.peek() != Some(&Tok::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if self.peek() == Some(&Tok::Comma) { self.pos += 1; continue; }
                        break;
                    }
                }
                self.eat(&Tok::RBracket)?;
                Ok(Node::Array(items))
            }
            _ => Err(()),
        }
    }
}

// ── Evaluation ──────────────────────────────────────────────────────────────────

fn eval_node(node: &Node, ctx: &Value) -> Value {
    match node {
        Node::Lit(v) => v.clone(),
        Node::Var(name) => ctx.get(name).cloned().unwrap_or(Value::Null),
        Node::Array(items) => Value::Array(items.iter().map(|n| eval_node(n, ctx)).collect()),
        Node::Member(obj, name) => eval_node(obj, ctx).get(name).cloned().unwrap_or(Value::Null),
        Node::Index(obj, idx) => {
            let target = eval_node(obj, ctx);
            let i = eval_node(idx, ctx);
            match &i {
                Value::Number(n) => n.as_f64()
                    .filter(|f| *f >= 0.0)
                    .and_then(|f| target.get(f as usize).cloned())
                    .unwrap_or(Value::Null),
                Value::String(s) => target.get(s.as_str()).cloned().unwrap_or(Value::Null),
                _ => Value::Null,
            }
        }
        Node::Unary(op, operand) => {
            let v = eval_node(operand, ctx);
            match op {
                Tok::Minus => json!(-num(&v)),
                Tok::Not   => Value::Bool(!truthy(&v)),
                _ => Value::Null,
            }
        }
        Node::Ternary(c, a, b) => {
            if truthy(&eval_node(c, ctx)) { eval_node(a, ctx) } else { eval_node(b, ctx) }
        }
        Node::Binary(op, l, r) => {
            // Short-circuit for && / ||.
            match op {
                Tok::And => {
                    let lv = eval_node(l, ctx);
                    if !truthy(&lv) { return lv; }
                    return eval_node(r, ctx);
                }
                Tok::Or => {
                    let lv = eval_node(l, ctx);
                    if truthy(&lv) { return lv; }
                    return eval_node(r, ctx);
                }
                _ => {}
            }
            let a = eval_node(l, ctx);
            let b = eval_node(r, ctx);
            eval_binary(op, &a, &b)
        }
        Node::Call(name, args) => {
            let vals: Vec<Value> = args.iter().map(|n| eval_node(n, ctx)).collect();
            call_fn(name, &vals)
        }
    }
}

fn eval_binary(op: &Tok, a: &Value, b: &Value) -> Value {
    match op {
        Tok::Plus => {
            // Numeric add only when both coerce cleanly; otherwise string concat.
            match (as_num(a), as_num(b)) {
                (Some(x), Some(y)) if a.is_number() && b.is_number() => json!(x + y),
                _ => {
                    if a.is_number() && b.is_number() {
                        json!(num(a) + num(b))
                    } else {
                        Value::String(format!("{}{}", text(a), text(b)))
                    }
                }
            }
        }
        Tok::Minus   => json!(num(a) - num(b)),
        Tok::Star    => json!(num(a) * num(b)),
        Tok::Slash   => { let d = num(b); if d == 0.0 { Value::Null } else { json!(num(a) / d) } }
        Tok::Percent => { let d = num(b); if d == 0.0 { Value::Null } else { json!(num(a) % d) } }
        Tok::Eq => Value::Bool(loose_eq(a, b)),
        Tok::Ne => Value::Bool(!loose_eq(a, b)),
        Tok::Lt => Value::Bool(num(a) < num(b)),
        Tok::Gt => Value::Bool(num(a) > num(b)),
        Tok::Le => Value::Bool(num(a) <= num(b)),
        Tok::Ge => Value::Bool(num(a) >= num(b)),
        _ => Value::Null,
    }
}

/// Loose equality: exact JSON equality OR equal textual representation.
fn loose_eq(a: &Value, b: &Value) -> bool {
    if a == b { return true; }
    if let (Some(x), Some(y)) = (as_num(a), as_num(b)) {
        if a.is_number() || b.is_number() { return x == y; }
    }
    text(a) == text(b)
}

// ── Built-in functions ──────────────────────────────────────────────────────────

fn call_fn(name: &str, args: &[Value]) -> Value {
    let a0 = || args.first().cloned().unwrap_or(Value::Null);
    let a1 = || args.get(1).cloned().unwrap_or(Value::Null);
    let a2 = || args.get(2).cloned().unwrap_or(Value::Null);
    match name {
        // ── strings ──
        "upper" => Value::String(text(&a0()).to_uppercase()),
        "lower" => Value::String(text(&a0()).to_lowercase()),
        "trim"  => Value::String(text(&a0()).trim().to_string()),
        "replace" => Value::String(text(&a0()).replace(&text(&a1()), &text(&a2()))),
        "split" => {
            let sep = text(&a1());
            let parts: Vec<Value> = if sep.is_empty() {
                text(&a0()).chars().map(|c| Value::String(c.to_string())).collect()
            } else {
                text(&a0()).split(&sep).map(|s| Value::String(s.to_string())).collect()
            };
            Value::Array(parts)
        }
        "substring" => {
            let s: Vec<char> = text(&a0()).chars().collect();
            let start = (num(&a1()) as usize).min(s.len());
            let end = if args.len() >= 3 { (start + num(&a2()) as usize).min(s.len()) } else { s.len() };
            Value::String(s[start..end.max(start)].iter().collect())
        }
        "contains"   => Value::Bool(text(&a0()).contains(&text(&a1())) || array_contains(&a0(), &a1())),
        "startsWith" => Value::Bool(text(&a0()).starts_with(&text(&a1()))),
        "endsWith"   => Value::Bool(text(&a0()).ends_with(&text(&a1()))),
        "padStart"   => {
            let s = text(&a0());
            let width = num(&a1()) as usize;
            let pad = { let p = text(&a2()); if p.is_empty() { " ".to_string() } else { p } };
            let mut out = String::new();
            while out.chars().count() + s.chars().count() < width {
                out.push_str(&pad);
            }
            let need = width.saturating_sub(s.chars().count());
            let prefix: String = out.chars().take(need).collect();
            Value::String(format!("{prefix}{s}"))
        }
        "urlEncode" => Value::String(url_encode(&text(&a0()))),

        // ── numbers ──
        "number" => as_num(&a0()).map(|n| json!(n)).unwrap_or(Value::Null),
        "round"  => {
            let dp = if args.len() >= 2 { num(&a1()) as i32 } else { 0 };
            let factor = 10f64.powi(dp);
            json!((num(&a0()) * factor).round() / factor)
        }
        "floor" => json!(num(&a0()).floor()),
        "ceil"  => json!(num(&a0()).ceil()),
        "abs"   => json!(num(&a0()).abs()),
        "min"   => fold_nums(args, f64::INFINITY, f64::min),
        "max"   => fold_nums(args, f64::NEG_INFINITY, f64::max),
        "sum"   => { let ns = collect_nums(args); json!(ns.iter().sum::<f64>()) }
        "avg"   => { let ns = collect_nums(args); if ns.is_empty() { json!(0) } else { json!(ns.iter().sum::<f64>() / ns.len() as f64) } }

        // ── arrays / objects ──
        "length" => json!(len_of(&a0())),
        "first"  => a0().as_array().and_then(|a| a.first().cloned()).unwrap_or(Value::Null),
        "last"   => a0().as_array().and_then(|a| a.last().cloned()).unwrap_or(Value::Null),
        "keys"   => a0().as_object().map(|o| Value::Array(o.keys().map(|k| Value::String(k.clone())).collect())).unwrap_or(json!([])),
        "values" => a0().as_object().map(|o| Value::Array(o.values().cloned().collect())).unwrap_or(json!([])),
        "join"   => {
            let sep = text(&a1());
            let arr = a0();
            let parts: Vec<String> = arr.as_array().map(|a| a.iter().map(text).collect()).unwrap_or_default();
            Value::String(parts.join(&sep))
        }
        "reverse" => {
            if let Some(a) = a0().as_array() { let mut v = a.clone(); v.reverse(); Value::Array(v) }
            else { Value::String(text(&a0()).chars().rev().collect()) }
        }
        "unique" => {
            let mut seen = Vec::new();
            if let Some(a) = a0().as_array() { for v in a { if !seen.contains(v) { seen.push(v.clone()); } } }
            Value::Array(seen)
        }
        "slice" => {
            let arr = a0();
            if let Some(a) = arr.as_array() {
                let start = (num(&a1()) as usize).min(a.len());
                let end = if args.len() >= 3 { (num(&a2()) as usize).min(a.len()) } else { a.len() };
                Value::Array(a[start..end.max(start)].to_vec())
            } else { json!([]) }
        }

        // ── utility ──
        "default" => { let v = a0(); if is_empty(&v) { a1() } else { v } },
        "if"      => if truthy(&a0()) { a1() } else { a2() },
        "string"  => Value::String(text(&a0())),
        "stringify" => Value::String(serde_json::to_string(&a0()).unwrap_or_default()),
        "json"    => match a0() { Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::Null), other => other },
        "boolean" => Value::Bool(truthy(&a0())),
        "uuid"    => Value::String(uuid::Uuid::new_v4().to_string()),

        // ── dates (UTC, ISO 8601) ──
        "now"   => Value::String(Utc::now().to_rfc3339()),
        "today" => Value::String(Utc::now().format("%Y-%m-%d").to_string()),
        "timestamp" => json!(Utc::now().timestamp()),
        "year"  => json!(parse_date(&a0()).map(|d| d.year()).unwrap_or(0)),
        "month" => json!(parse_date(&a0()).map(|d| d.month()).unwrap_or(0)),
        "day"   => json!(parse_date(&a0()).map(|d| d.day()).unwrap_or(0)),
        "hour"  => json!(parse_date(&a0()).map(|d| d.hour()).unwrap_or(0)),
        "dateFormat" => {
            match parse_date(&a0()) {
                Some(d) => Value::String(d.format(&text(&a1())).to_string()),
                None => Value::Null,
            }
        }
        "dateAdd" => {
            // dateAdd(date, n, unit) — unit: seconds|minutes|hours|days
            match parse_date(&a0()) {
                Some(d) => {
                    let n = num(&a1()) as i64;
                    let unit = text(&a2());
                    let delta = match unit.as_str() {
                        "seconds" | "second" => Duration::seconds(n),
                        "minutes" | "minute" => Duration::minutes(n),
                        "hours"   | "hour"   => Duration::hours(n),
                        _ /* days */         => Duration::days(n),
                    };
                    Value::String((d + delta).to_rfc3339())
                }
                None => Value::Null,
            }
        }
        "dateDiff" => {
            // dateDiff(a, b, unit) → a - b in unit (default seconds)
            match (parse_date(&a0()), parse_date(&a1())) {
                (Some(x), Some(y)) => {
                    let secs = (x - y).num_seconds() as f64;
                    let unit = if args.len() >= 3 { text(&a2()) } else { "seconds".into() };
                    let v = match unit.as_str() {
                        "minutes" | "minute" => secs / 60.0,
                        "hours"   | "hour"   => secs / 3600.0,
                        "days"    | "day"    => secs / 86400.0,
                        _ => secs,
                    };
                    json!(v)
                }
                _ => Value::Null,
            }
        }

        _ => Value::Null, // unknown function → null (forgiving)
    }
}

// ── Value helpers ───────────────────────────────────────────────────────────────

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

fn as_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn num(v: &Value) -> f64 { as_num(v).unwrap_or(0.0) }

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn len_of(v: &Value) -> usize {
    match v {
        Value::String(s) => s.chars().count(),
        Value::Array(a) => a.len(),
        Value::Object(o) => o.len(),
        _ => 0,
    }
}

fn array_contains(haystack: &Value, needle: &Value) -> bool {
    haystack.as_array().map(|a| a.iter().any(|x| loose_eq(x, needle))).unwrap_or(false)
}

fn collect_nums(args: &[Value]) -> Vec<f64> {
    // Accept either a single array argument or a varargs list of numbers.
    if args.len() == 1 {
        if let Some(a) = args[0].as_array() {
            return a.iter().filter_map(as_num).collect();
        }
    }
    args.iter().filter_map(as_num).collect()
}

fn fold_nums(args: &[Value], init: f64, f: fn(f64, f64) -> f64) -> Value {
    let ns = collect_nums(args);
    if ns.is_empty() { return Value::Null; }
    json!(ns.into_iter().fold(init, f))
}

fn parse_date(v: &Value) -> Option<chrono::DateTime<Utc>> {
    match v {
        Value::String(s) => {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                return Some(dt.with_timezone(&Utc));
            }
            if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                return d.and_hms_opt(0, 0, 0).map(|nd| chrono::DateTime::<Utc>::from_naive_utc_and_offset(nd, Utc));
            }
            None
        }
        Value::Number(n) => n.as_i64().and_then(|ts| chrono::DateTime::<Utc>::from_timestamp(ts, 0)),
        _ => None,
    }
}

/// Minimal percent-encoding for query components (no external dep).
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Catalogue of expression functions, surfaced to the frontend for autocomplete.
pub fn function_catalog() -> Value {
    let f = |name: &str, sig: &str, desc: &str| json!({ "name": name, "signature": sig, "description": desc });
    json!([
        f("upper", "upper(text)", "Met le texte en MAJUSCULES"),
        f("lower", "lower(text)", "Met le texte en minuscules"),
        f("trim", "trim(text)", "Retire les espaces de début/fin"),
        f("replace", "replace(text, cherche, remplace)", "Remplace toutes les occurrences"),
        f("split", "split(text, séparateur)", "Découpe une chaîne en tableau"),
        f("join", "join(tableau, séparateur)", "Assemble un tableau en chaîne"),
        f("substring", "substring(text, début, longueur?)", "Extrait une sous-chaîne"),
        f("contains", "contains(valeur, recherche)", "Vrai si contient (texte ou tableau)"),
        f("startsWith", "startsWith(text, préfixe)", "Vrai si commence par"),
        f("endsWith", "endsWith(text, suffixe)", "Vrai si termine par"),
        f("padStart", "padStart(text, largeur, char)", "Complète à gauche"),
        f("urlEncode", "urlEncode(text)", "Encode pour une URL"),
        f("length", "length(valeur)", "Longueur d'un texte/tableau/objet"),
        f("number", "number(valeur)", "Convertit en nombre"),
        f("round", "round(nombre, décimales?)", "Arrondit"),
        f("floor", "floor(nombre)", "Arrondit à l'entier inférieur"),
        f("ceil", "ceil(nombre)", "Arrondit à l'entier supérieur"),
        f("abs", "abs(nombre)", "Valeur absolue"),
        f("min", "min(a, b, …)", "Minimum"),
        f("max", "max(a, b, …)", "Maximum"),
        f("sum", "sum(tableau)", "Somme"),
        f("avg", "avg(tableau)", "Moyenne"),
        f("first", "first(tableau)", "Premier élément"),
        f("last", "last(tableau)", "Dernier élément"),
        f("keys", "keys(objet)", "Clés d'un objet"),
        f("values", "values(objet)", "Valeurs d'un objet"),
        f("reverse", "reverse(tableau)", "Inverse"),
        f("unique", "unique(tableau)", "Dédoublonne"),
        f("slice", "slice(tableau, début, fin?)", "Sous-tableau"),
        f("default", "default(valeur, repli)", "Valeur de repli si vide"),
        f("if", "if(condition, alors, sinon)", "Condition en ligne"),
        f("string", "string(valeur)", "Convertit en texte"),
        f("stringify", "stringify(valeur)", "Sérialise en JSON"),
        f("json", "json(text)", "Parse du JSON"),
        f("uuid", "uuid()", "Identifiant unique"),
        f("now", "now()", "Date/heure courante (ISO)"),
        f("today", "today()", "Date du jour (AAAA-MM-JJ)"),
        f("timestamp", "timestamp()", "Horodatage Unix"),
        f("dateFormat", "dateFormat(date, format)", "Formate une date (strftime)"),
        f("dateAdd", "dateAdd(date, n, unité)", "Ajoute une durée (days/hours/…)"),
        f("dateDiff", "dateDiff(a, b, unité)", "Différence entre deux dates"),
    ])
}

/// Special context variables, surfaced to the frontend for autocomplete.
pub fn variable_catalog() -> Value {
    json!([
        { "name": "$json",      "description": "Données entrantes du nœud" },
        { "name": "$input",     "description": "Alias de $json" },
        { "name": "trigger",    "description": "Données du déclencheur" },
        { "name": "nodes",      "description": "Sorties des nœuds précédents (nodes.<id>)" },
        { "name": "$now",       "description": "Date/heure courante (ISO)" },
        { "name": "$today",     "description": "Date du jour" },
        { "name": "$workflow",  "description": "Métadonnées du workflow (id, name)" },
        { "name": "$execution", "description": "Métadonnées de l'exécution (id, mode)" },
        { "name": "$vars",      "description": "Variables définies dans le workflow" },
    ])
}

/// Inject the special `$now` / `$today` variables into a resolution context.
/// Other vars (`$json`, `$workflow`, …) are injected by the executor.
pub fn with_now(ctx: &mut Map<String, Value>) {
    let now = Utc::now();
    ctx.entry("$now".to_string()).or_insert_with(|| Value::String(now.to_rfc3339()));
    ctx.entry("$today".to_string()).or_insert_with(|| Value::String(now.format("%Y-%m-%d").to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str, ctx: Value) -> Value { evaluate(s, &ctx) }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("2 + 3 * 4", json!({})), json!(14.0));
        assert_eq!(ev("(2 + 3) * 4", json!({})), json!(20.0));
        assert_eq!(ev("10 % 3", json!({})), json!(1.0));
        assert_eq!(ev("-5 + 2", json!({})), json!(-3.0));
    }

    #[test]
    fn comparisons_and_logic() {
        assert_eq!(ev("3 > 2 && 1 < 2", json!({})), json!(true));
        assert_eq!(ev("3 > 5 || 1 == 1", json!({})), json!(true));
        assert_eq!(ev("!false", json!({})), json!(true));
        assert_eq!(ev("5 >= 5", json!({})), json!(true));
    }

    #[test]
    fn ternary() {
        assert_eq!(ev("3 > 2 ? 'oui' : 'non'", json!({})), json!("oui"));
        assert_eq!(ev("trigger.n > 10 ? 'grand' : 'petit'", json!({"trigger":{"n":5}})), json!("petit"));
    }

    #[test]
    fn variable_and_member_access() {
        let ctx = json!({ "trigger": { "user": { "name": "Bob" } }, "items": [{"id": 1}, {"id": 2}] });
        assert_eq!(ev("trigger.user.name", ctx.clone()), json!("Bob"));
        assert_eq!(ev("items[1].id", ctx.clone()), json!(2));
        assert_eq!(ev("items[0].id + items[1].id", ctx), json!(3.0));
    }

    #[test]
    fn string_concat_and_funcs() {
        let ctx = json!({ "trigger": { "first": "Ada", "last": "Lovelace" } });
        assert_eq!(ev("trigger.first + ' ' + trigger.last", ctx.clone()), json!("Ada Lovelace"));
        assert_eq!(ev("upper(trigger.first)", ctx.clone()), json!("ADA"));
        assert_eq!(ev("length(trigger.last)", ctx), json!(8));
    }

    #[test]
    fn defaults_and_unknown() {
        assert_eq!(ev("default(trigger.missing, 'fallback')", json!({"trigger":{}})), json!("fallback"));
        assert_eq!(ev("unknownThing", json!({})), Value::Null);
        assert_eq!(ev("nope(1,2)", json!({})), Value::Null);
    }

    #[test]
    fn arrays_and_aggregates() {
        let ctx = json!({ "xs": [3, 1, 2] });
        assert_eq!(ev("sum(xs)", ctx.clone()), json!(6.0));
        assert_eq!(ev("max(xs)", ctx.clone()), json!(3.0));
        assert_eq!(ev("join(xs, '-')", ctx.clone()), json!("3-1-2"));
        assert_eq!(ev("length(xs)", ctx), json!(3));
    }

    #[test]
    fn round_and_number() {
        assert_eq!(ev("round(3.14159, 2)", json!({})), json!(3.14));
        assert_eq!(ev("number('42') + 1", json!({})), json!(43.0));
    }

    #[test]
    fn malformed_is_null() {
        assert_eq!(ev("2 +", json!({})), Value::Null);
        assert_eq!(ev("((", json!({})), Value::Null);
        assert_eq!(ev("'unterminated", json!({})), Value::Null);
    }
}
