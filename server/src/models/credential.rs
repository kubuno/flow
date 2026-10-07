//! Reusable credentials: catalogue of credential *types* (like node metadata, but
//! for authentication) + the stored credential row and DTOs.
//!
//! The catalogue is intentionally built from generic primitives (HTTP Basic /
//! Header / Query / Bearer auth, OAuth1/OAuth2, generic API key) which make *any*
//! HTTP API usable, plus a broad curated set of popular services. New types are
//! pure data — add an entry to `credential_catalog()`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

// ── Type catalogue (exposed to the frontend) ────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CredFieldType {
    Text,
    Password,
    Select,
    Boolean,
    Number,
    Json,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredFieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredField {
    pub name:        String,
    pub label:       String,
    #[serde(rename = "type")]
    pub field_type:  CredFieldType,
    #[serde(default)]
    pub required:    bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help:        Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default:     Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options:     Option<Vec<CredFieldOption>>,
}

impl CredField {
    fn new(name: &str, label: &str, ft: CredFieldType) -> Self {
        Self { name: name.into(), label: label.into(), field_type: ft, required: false, placeholder: None, help: None, default: None, options: None }
    }
    fn req(mut self) -> Self { self.required = true; self }
    fn ph(mut self, p: &str) -> Self { self.placeholder = Some(p.into()); self }
    fn help(mut self, h: &str) -> Self { self.help = Some(h.into()); self }
    fn default(mut self, v: Value) -> Self { self.default = Some(v); self }
    fn options(mut self, opts: &[(&str, &str)]) -> Self {
        self.options = Some(opts.iter().map(|(v, l)| CredFieldOption { value: v.to_string(), label: l.to_string() }).collect());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialType {
    #[serde(rename = "type")]
    pub type_id:  String,
    pub name:     String,
    pub icon:     String,
    pub category: String,
    pub fields:   Vec<CredField>,
}

// ── Compact field/type constructors ─────────────────────────────────────────────

fn txt(name: &str, label: &str) -> CredField { CredField::new(name, label, CredFieldType::Text) }
fn pwd(name: &str, label: &str) -> CredField { CredField::new(name, label, CredFieldType::Password) }

fn ct(type_id: &str, name: &str, icon: &str, category: &str, fields: Vec<CredField>) -> CredentialType {
    CredentialType { type_id: type_id.into(), name: name.into(), icon: icon.into(), category: category.into(), fields }
}

/// Single-secret credential (API key in a password field named `apiKey`).
fn api_key(type_id: &str, name: &str, icon: &str, category: &str) -> CredentialType {
    ct(type_id, name, icon, category, vec![pwd("apiKey", "Clé API").req()])
}
/// Single-secret credential whose field is named `accessToken`.
fn token(type_id: &str, name: &str, icon: &str, category: &str) -> CredentialType {
    ct(type_id, name, icon, category, vec![pwd("accessToken", "Jeton d'accès").req()])
}
/// OAuth2 client credentials (clientId + clientSecret, optional scope).
fn oauth2(type_id: &str, name: &str, icon: &str, category: &str) -> CredentialType {
    ct(type_id, name, icon, category, vec![
        txt("clientId", "Client ID").req(),
        pwd("clientSecret", "Client Secret").req(),
        txt("scope", "Portées (scopes)"),
    ])
}

/// The full credential type catalogue.
#[allow(clippy::vec_init_then_push)]
pub fn credential_catalog() -> Vec<CredentialType> {
    let mut v = Vec::new();

    // ── Generic HTTP auth primitives (cover ANY REST API) ──
    v.push(ct("httpBasicAuth", "HTTP Basic Auth", "Lock", "generic", vec![
        txt("user", "Utilisateur").req(), pwd("password", "Mot de passe").req(),
    ]));
    v.push(ct("httpHeaderAuth", "HTTP — En-tête", "Lock", "generic", vec![
        txt("name", "Nom de l'en-tête").req().ph("Authorization"), pwd("value", "Valeur").req(),
    ]));
    v.push(ct("httpQueryAuth", "HTTP — Paramètre d'URL", "Lock", "generic", vec![
        txt("name", "Nom du paramètre").req().ph("api_key"), pwd("value", "Valeur").req(),
    ]));
    v.push(ct("httpBearerAuth", "HTTP — Bearer", "Lock", "generic", vec![
        pwd("token", "Jeton (Bearer)").req(),
    ]));
    v.push(ct("httpDigestAuth", "HTTP Digest Auth", "Lock", "generic", vec![
        txt("user", "Utilisateur").req(), pwd("password", "Mot de passe").req(),
    ]));
    v.push(ct("httpCustomAuth", "HTTP — Personnalisé (JSON)", "Lock", "generic", vec![
        CredField::new("json", "En-têtes / paramètres (JSON)", CredFieldType::Json).req()
            .help(r#"{"headers":{"X-Api-Key":"…"},"qs":{"token":"…"}}"#),
    ]));
    v.push(ct("oAuth2Api", "OAuth2 (générique)", "KeyRound", "generic", vec![
        CredField::new("grantType", "Type d'octroi", CredFieldType::Select)
            .options(&[("clientCredentials","Client Credentials"),("authorizationCode","Authorization Code"),("pkce","PKCE")]).default(serde_json::json!("clientCredentials")),
        txt("authUrl", "URL d'autorisation"),
        txt("accessTokenUrl", "URL du jeton d'accès").req(),
        txt("clientId", "Client ID").req(),
        pwd("clientSecret", "Client Secret").req(),
        txt("scope", "Portées (scopes)"),
        CredField::new("authentication", "Envoi des identifiants", CredFieldType::Select)
            .options(&[("header","En-tête (Basic)"),("body","Corps")]).default(serde_json::json!("header")),
    ]));
    v.push(ct("oAuth1Api", "OAuth1 (générique)", "KeyRound", "generic", vec![
        txt("consumerKey", "Consumer Key").req(),
        pwd("consumerSecret", "Consumer Secret").req(),
        pwd("accessToken", "Access Token"),
        pwd("accessTokenSecret", "Access Token Secret"),
        CredField::new("signatureMethod", "Méthode de signature", CredFieldType::Select)
            .options(&[("HMAC-SHA1","HMAC-SHA1"),("HMAC-SHA256","HMAC-SHA256"),("PLAINTEXT","PLAINTEXT")]).default(serde_json::json!("HMAC-SHA1")),
    ]));
    v.push(api_key("apiKeyAuth", "Clé API (générique)", "Key", "generic"));

    // ── Databases ──
    let db_fields = |default_port: i64| vec![
        txt("host", "Hôte").req().ph("localhost"),
        CredField::new("port", "Port", CredFieldType::Number).default(serde_json::json!(default_port)),
        txt("database", "Base de données").req(),
        txt("user", "Utilisateur").req(),
        pwd("password", "Mot de passe"),
        CredField::new("ssl", "SSL", CredFieldType::Boolean).default(serde_json::json!(false)),
    ];
    v.push(ct("postgres", "PostgreSQL", "Database", "database", db_fields(5432)));
    v.push(ct("mysql", "MySQL", "Database", "database", db_fields(3306)));
    v.push(ct("mariaDb", "MariaDB", "Database", "database", db_fields(3306)));
    v.push(ct("microsoftSql", "Microsoft SQL", "Database", "database", db_fields(1433)));
    v.push(ct("cockroachDb", "CockroachDB", "Database", "database", db_fields(26257)));
    v.push(ct("redis", "Redis", "Database", "database", vec![
        txt("host", "Hôte").req().ph("localhost"), CredField::new("port", "Port", CredFieldType::Number).default(serde_json::json!(6379)),
        pwd("password", "Mot de passe"), CredField::new("database", "Base (index)", CredFieldType::Number).default(serde_json::json!(0)),
    ]));
    v.push(ct("mongoDb", "MongoDB", "Database", "database", vec![pwd("connectionString", "Chaîne de connexion").req().ph("mongodb://…")]));
    v.push(ct("snowflake", "Snowflake", "Database", "database", vec![
        txt("account", "Compte").req(), txt("database", "Base").req(), txt("warehouse", "Warehouse"),
        txt("user", "Utilisateur").req(), pwd("password", "Mot de passe").req(),
    ]));
    v.push(ct("elasticsearch", "Elasticsearch", "Database", "database", vec![
        txt("baseUrl", "URL").req().ph("https://localhost:9200"), txt("username", "Utilisateur"), pwd("password", "Mot de passe"),
    ]));
    v.push(ct("influxDb", "InfluxDB", "Database", "database", vec![
        txt("url", "URL").req().ph("http://localhost:8086"), pwd("token", "Jeton").req(), txt("org", "Organisation"),
    ]));
    v.push(ct("nextcloud", "Nextcloud", "Cloud", "cloud", vec![
        txt("host", "URL de l'instance").req().ph("https://cloud.exemple.com"),
        txt("username", "Utilisateur").req(),
        pwd("appPassword", "Mot de passe d'application").req(),
    ]));
    v.push(ct("googleAccessToken", "Google (jeton OAuth2)", "Globe", "google", vec![
        pwd("accessToken", "Jeton d'accès OAuth2").req().help("Collez un access token Google (le flux OAuth n'est pas géré)."),
    ]));
    v.push(ct("microsoftAccessToken", "Microsoft (jeton OAuth2)", "Globe", "microsoft", vec![
        pwd("accessToken", "Jeton d'accès OAuth2").req(),
    ]));
    v.push(ct("supabase", "Supabase", "Database", "database", vec![
        txt("host", "URL du projet").req().ph("https://xxxx.supabase.co"), pwd("serviceRole", "Clé service role").req(),
    ]));

    // ── Email / SMTP ──
    v.push(ct("smtp", "SMTP", "Mail", "email", vec![
        txt("host", "Hôte").req(), CredField::new("port", "Port", CredFieldType::Number).default(serde_json::json!(587)),
        txt("user", "Utilisateur"), pwd("password", "Mot de passe"),
        CredField::new("secure", "TLS/SSL", CredFieldType::Boolean).default(serde_json::json!(true)),
    ]));
    v.push(ct("imap", "IMAP", "Mail", "email", vec![
        txt("host", "Hôte").req(), CredField::new("port", "Port", CredFieldType::Number).default(serde_json::json!(993)),
        txt("user", "Utilisateur").req(), pwd("password", "Mot de passe").req(),
        CredField::new("secure", "TLS/SSL", CredFieldType::Boolean).default(serde_json::json!(true)),
    ]));

    // ── AI / LLM ──
    v.push(api_key("anthropicApi", "Anthropic (Claude)", "Sparkles", "ai"));
    v.push(api_key("openAiApi", "OpenAI", "Sparkles", "ai"));
    v.push(api_key("mistralApi", "Mistral AI", "Sparkles", "ai"));
    v.push(api_key("cohereApi", "Cohere", "Sparkles", "ai"));
    v.push(api_key("googleGeminiApi", "Google Gemini", "Sparkles", "ai"));
    v.push(api_key("groqApi", "Groq", "Sparkles", "ai"));
    v.push(api_key("perplexityApi", "Perplexity", "Sparkles", "ai"));
    v.push(api_key("huggingFaceApi", "Hugging Face", "Sparkles", "ai"));
    v.push(api_key("deepLApi", "DeepL", "Sparkles", "ai"));
    v.push(api_key("elevenLabsApi", "ElevenLabs", "Sparkles", "ai"));
    v.push(api_key("stabilityAiApi", "Stability AI", "Sparkles", "ai"));
    v.push(api_key("replicateApi", "Replicate", "Sparkles", "ai"));
    v.push(ct("ollamaApi", "Ollama", "Sparkles", "ai", vec![txt("baseUrl", "URL de base").req().ph("http://localhost:11434")]));
    v.push(ct("azureOpenAiApi", "Azure OpenAI", "Sparkles", "ai", vec![
        txt("endpoint", "Endpoint").req().ph("https://xxx.openai.azure.com"), pwd("apiKey", "Clé API").req(),
    ]));

    // ── Messaging / Chat ──
    v.push(token("slackApi", "Slack", "MessageSquare", "messaging"));
    v.push(oauth2("slackOAuth2Api", "Slack OAuth2", "MessageSquare", "messaging"));
    v.push(ct("discordApi", "Discord (Bot)", "MessageSquare", "messaging", vec![pwd("botToken", "Jeton du bot").req()]));
    v.push(ct("discordWebhook", "Discord (Webhook)", "MessageSquare", "messaging", vec![txt("webhookUrl", "URL du webhook").req()]));
    v.push(ct("telegramApi", "Telegram (Bot)", "Send", "messaging", vec![pwd("accessToken", "Jeton du bot").req()]));
    v.push(ct("mattermostApi", "Mattermost", "MessageSquare", "messaging", vec![txt("baseUrl", "URL").req(), pwd("accessToken", "Jeton").req()]));
    v.push(ct("rocketChatApi", "Rocket.Chat", "MessageSquare", "messaging", vec![txt("domain", "Domaine").req(), txt("userId", "User ID").req(), pwd("authKey", "Auth Token").req()]));
    v.push(oauth2("microsoftTeamsOAuth2Api", "Microsoft Teams", "MessageSquare", "messaging"));
    v.push(ct("twilioApi", "Twilio", "Phone", "messaging", vec![txt("accountSid", "Account SID").req(), pwd("authToken", "Auth Token").req()]));
    v.push(ct("vonageApi", "Vonage", "Phone", "messaging", vec![txt("apiKey", "API Key").req(), pwd("apiSecret", "API Secret").req()]));
    v.push(ct("whatsAppApi", "WhatsApp Business", "MessageSquare", "messaging", vec![pwd("accessToken", "Jeton").req(), txt("phoneNumberId", "Phone Number ID").req()]));
    v.push(ct("matrixApi", "Matrix", "MessageSquare", "messaging", vec![txt("homeserverUrl", "Homeserver").req(), pwd("accessToken", "Jeton").req()]));

    // ── Dev / DevOps ──
    v.push(token("githubApi", "GitHub", "Github", "dev"));
    v.push(oauth2("githubOAuth2Api", "GitHub OAuth2", "Github", "dev"));
    v.push(token("gitlabApi", "GitLab", "Gitlab", "dev"));
    v.push(ct("bitbucketApi", "Bitbucket", "GitBranch", "dev", vec![txt("username", "Utilisateur").req(), pwd("appPassword", "App Password").req()]));
    v.push(ct("jiraApi", "Jira", "SquareKanban", "dev", vec![txt("domain", "Domaine").req().ph("https://xxx.atlassian.net"), txt("email", "E-mail").req(), pwd("apiToken", "Jeton API").req()]));
    v.push(ct("confluenceApi", "Confluence", "FileText", "dev", vec![txt("domain", "Domaine").req(), txt("email", "E-mail").req(), pwd("apiToken", "Jeton API").req()]));
    v.push(api_key("sentryApi", "Sentry", "Bug", "dev"));
    v.push(ct("grafanaApi", "Grafana", "ChartArea", "dev", vec![txt("baseUrl", "URL").req(), pwd("apiKey", "Clé API").req()]));
    v.push(ct("datadogApi", "Datadog", "ChartArea", "dev", vec![pwd("apiKey", "API Key").req(), pwd("appKey", "Application Key").req()]));
    v.push(api_key("pagerDutyApi", "PagerDuty", "Siren", "dev"));
    v.push(api_key("opsgenieApi", "Opsgenie", "Siren", "dev"));
    v.push(api_key("npmApi", "npm", "Package", "dev"));
    v.push(ct("dockerHubApi", "Docker Hub", "Container", "dev", vec![txt("username", "Utilisateur").req(), pwd("accessToken", "Jeton").req()]));

    // ── Productivity / PM ──
    v.push(token("notionApi", "Notion", "FileText", "productivity"));
    v.push(ct("airtableApi", "Airtable", "Table", "productivity", vec![pwd("apiKey", "Personal Access Token").req()]));
    v.push(api_key("codaApi", "Coda", "FileText", "productivity"));
    v.push(api_key("clickUpApi", "ClickUp", "SquareKanban", "productivity"));
    v.push(api_key("asanaApi", "Asana", "SquareKanban", "productivity"));
    v.push(ct("trelloApi", "Trello", "SquareKanban", "productivity", vec![txt("apiKey", "API Key").req(), pwd("apiToken", "API Token").req()]));
    v.push(api_key("mondayComApi", "monday.com", "SquareKanban", "productivity"));
    v.push(api_key("todoistApi", "Todoist", "ListChecks", "productivity"));
    v.push(api_key("linearApi", "Linear", "SquareKanban", "productivity"));

    // ── Google / Microsoft ──
    v.push(ct("googleApi", "Google (Compte de service)", "Globe", "google", vec![
        txt("email", "E-mail du compte de service").req(),
        CredField::new("privateKey", "Clé privée", CredFieldType::Password).req().help("Contenu PEM de la clé privée"),
    ]));
    v.push(oauth2("googleOAuth2Api", "Google OAuth2 (Sheets/Drive/Gmail/Agenda)", "Globe", "google"));
    v.push(oauth2("microsoftOAuth2Api", "Microsoft OAuth2 (Graph)", "Globe", "microsoft"));

    // ── Cloud / Storage ──
    v.push(ct("aws", "AWS", "Cloud", "cloud", vec![
        txt("region", "Région").req().ph("eu-west-3"), txt("accessKeyId", "Access Key ID").req(),
        pwd("secretAccessKey", "Secret Access Key").req(), pwd("sessionToken", "Session Token"),
    ]));
    v.push(ct("s3", "S3 (compatible)", "Cloud", "cloud", vec![
        txt("endpoint", "Endpoint").ph("https://s3.amazonaws.com"), txt("region", "Région"),
        txt("accessKeyId", "Access Key ID").req(), pwd("secretAccessKey", "Secret Access Key").req(),
    ]));
    v.push(token("dropboxApi", "Dropbox", "Cloud", "cloud"));
    v.push(token("boxApi", "Box", "Cloud", "cloud"));
    v.push(api_key("cloudflareApi", "Cloudflare", "Cloud", "cloud"));
    v.push(api_key("digitalOceanApi", "DigitalOcean", "Cloud", "cloud"));
    v.push(api_key("hetznerApi", "Hetzner", "Cloud", "cloud"));
    v.push(api_key("vercelApi", "Vercel", "Triangle", "cloud"));
    v.push(api_key("netlifyApi", "Netlify", "Cloud", "cloud"));

    // ── CRM / Marketing / Email ──
    v.push(api_key("hubspotApi", "HubSpot", "Contact", "crm"));
    v.push(ct("salesforceApi", "Salesforce", "Cloud", "crm", vec![txt("instanceUrl", "Instance URL").req(), pwd("accessToken", "Jeton").req()]));
    v.push(api_key("pipedriveApi", "Pipedrive", "Contact", "crm"));
    v.push(api_key("intercomApi", "Intercom", "MessageSquare", "crm"));
    v.push(ct("zendeskApi", "Zendesk", "MessageSquare", "crm", vec![txt("subdomain", "Sous-domaine").req(), txt("email", "E-mail").req(), pwd("apiToken", "Jeton API").req()]));
    v.push(api_key("freshdeskApi", "Freshdesk", "MessageSquare", "crm"));
    v.push(ct("mailchimpApi", "Mailchimp", "Mail", "marketing", vec![pwd("apiKey", "Clé API").req(), txt("server", "Préfixe serveur").ph("us21")]));
    v.push(ct("mailgunApi", "Mailgun", "Mail", "marketing", vec![pwd("apiKey", "Clé API").req(), txt("domain", "Domaine").req()]));
    v.push(api_key("sendGridApi", "SendGrid", "Mail", "marketing"));
    v.push(ct("mailjetApi", "Mailjet", "Mail", "marketing", vec![txt("apiKey", "API Key").req(), pwd("apiSecret", "Secret Key").req()]));
    v.push(api_key("brevoApi", "Brevo (Sendinblue)", "Mail", "marketing"));
    v.push(api_key("postmarkApi", "Postmark", "Mail", "marketing"));
    v.push(api_key("klaviyoApi", "Klaviyo", "Mail", "marketing"));
    v.push(api_key("activeCampaignApi", "ActiveCampaign", "Mail", "marketing"));

    // ── Payments / Commerce ──
    v.push(api_key("stripeApi", "Stripe", "CreditCard", "commerce"));
    v.push(ct("paypalApi", "PayPal", "CreditCard", "commerce", vec![txt("clientId", "Client ID").req(), pwd("clientSecret", "Secret").req(), CredField::new("environment", "Environnement", CredFieldType::Select).options(&[("live","Production"),("sandbox","Sandbox")]).default(serde_json::json!("live"))]));
    v.push(ct("shopifyApi", "Shopify", "ShoppingCart", "commerce", vec![txt("shopSubdomain", "Sous-domaine").req(), pwd("accessToken", "Jeton").req()]));
    v.push(ct("wooCommerceApi", "WooCommerce", "ShoppingCart", "commerce", vec![txt("url", "URL").req(), txt("consumerKey", "Consumer Key").req(), pwd("consumerSecret", "Consumer Secret").req()]));
    v.push(api_key("lemonSqueezyApi", "Lemon Squeezy", "CreditCard", "commerce"));
    v.push(api_key("paddleApi", "Paddle", "CreditCard", "commerce"));

    // ── Social ──
    v.push(oauth2("twitterOAuth2Api", "X (Twitter)", "Twitter", "social"));
    v.push(oauth2("linkedInOAuth2Api", "LinkedIn", "Linkedin", "social"));
    v.push(oauth2("facebookGraphApi", "Facebook", "Facebook", "social"));
    v.push(oauth2("youTubeOAuth2Api", "YouTube", "Youtube", "social"));
    v.push(oauth2("redditOAuth2Api", "Reddit", "Globe", "social"));
    v.push(oauth2("spotifyOAuth2Api", "Spotify", "Music", "social"));
    v.push(ct("mastodonApi", "Mastodon", "Globe", "social", vec![txt("url", "Instance").req(), pwd("accessToken", "Jeton").req()]));

    // ── CMS ──
    v.push(ct("wordpressApi", "WordPress", "FileText", "cms", vec![txt("url", "URL").req(), txt("username", "Utilisateur").req(), pwd("password", "App Password").req()]));
    v.push(ct("ghostApi", "Ghost", "FileText", "cms", vec![txt("url", "URL").req(), pwd("adminApiKey", "Admin API Key").req()]));
    v.push(api_key("webflowApi", "Webflow", "Globe", "cms"));
    v.push(ct("contentfulApi", "Contentful", "FileText", "cms", vec![txt("spaceId", "Space ID").req(), pwd("accessToken", "Jeton").req()]));
    v.push(ct("strapiApi", "Strapi", "FileText", "cms", vec![txt("url", "URL").req(), pwd("apiToken", "Jeton API").req()]));

    // ── Vector / Search ──
    v.push(ct("pineconeApi", "Pinecone", "Boxes", "search", vec![pwd("apiKey", "Clé API").req(), txt("environment", "Environnement")]));
    v.push(ct("weaviateApi", "Weaviate", "Boxes", "search", vec![txt("host", "Hôte").req(), pwd("apiKey", "Clé API")]));
    v.push(ct("qdrantApi", "Qdrant", "Boxes", "search", vec![txt("url", "URL").req(), pwd("apiKey", "Clé API")]));
    v.push(ct("algoliaApi", "Algolia", "Search", "search", vec![txt("applicationId", "Application ID").req(), pwd("apiKey", "Clé API").req()]));
    v.push(ct("meilisearchApi", "Meilisearch", "Search", "search", vec![txt("host", "Hôte").req(), pwd("apiKey", "Clé API")]));
    v.push(ct("typesenseApi", "Typesense", "Search", "search", vec![txt("host", "Hôte").req(), pwd("apiKey", "Clé API").req()]));

    // ── Misc APIs ──
    v.push(api_key("openWeatherMapApi", "OpenWeatherMap", "CloudSun", "misc"));
    v.push(api_key("googleMapsApi", "Google Maps", "Map", "misc"));
    v.push(api_key("mapboxApi", "Mapbox", "Map", "misc"));
    v.push(api_key("newsApi", "News API", "Newspaper", "misc"));
    v.push(api_key("nasaApi", "NASA", "Rocket", "misc"));
    v.push(api_key("calendlyApi", "Calendly", "Calendar", "misc"));
    v.push(oauth2("zoomOAuth2Api", "Zoom", "Video", "misc"));

    v
}

// ── DB row & DTOs ───────────────────────────────────────────────────────────────

/// `flow.credentials` row. The encrypted `data`/`nonce` are NOT serialized.
#[derive(Debug, Clone, FromRow)]
pub struct Credential {
    pub id:         Uuid,
    pub owner_id:   Uuid,
    pub name:       String,
    #[sqlx(rename = "type")]
    pub type_id:    String,
    pub data:       Vec<u8>,
    pub nonce:      Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Public view of a credential (metadata only — never the secret values).
#[derive(Debug, Serialize)]
pub struct CredentialMeta {
    pub id:         Uuid,
    pub name:       String,
    #[serde(rename = "type")]
    pub type_id:    String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Credential> for CredentialMeta {
    fn from(c: &Credential) -> Self {
        Self { id: c.id, name: c.name.clone(), type_id: c.type_id.clone(), created_at: c.created_at, updated_at: c.updated_at }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateCredentialDto {
    pub name: String,
    #[serde(rename = "type")]
    pub type_id: String,
    /// JSON object of field → value (plaintext, encrypted before storage).
    pub data: Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCredentialDto {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
}
