pub struct Metadata {
    pub client: String,
    pub authenticated_user: Option<String>,
    pub from: String,
    pub recipients: Vec<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub date: Option<String>,
    pub message_id: Option<String>,
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            client: String::new(),
            authenticated_user: None,
            from: String::new(),
            recipients: vec![],
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: String::new(),
            date: None,
            message_id: None,
        }
    }
}

pub enum AuthState {
    AwaithAuthRequest,
    RequestingUsername,
    RequestingPassword,
}
pub enum State {
    Initialized,
    AwaitingAuth {
        state: AuthState,
        username: Option<String>,
    },
    Authenticated,
    ProvidingHeaders,
    ProvidingData,
    Quitting,
}
