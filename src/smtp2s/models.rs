use serde::Serialize;

#[derive(Default, Serialize, Debug)]
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

pub enum AuthState {
    AwaithAuthRequest,
    RequestingUsername,
    RequestingPassword,
}

pub enum HeadersState {
    ProvidingFrom,
    ProvidingRecipients,
}

pub enum State {
    Initialized,
    Authenticating {
        state: AuthState,
        username: Option<String>,
    },
    ProvidingHeaders {
        state: HeadersState,
    },
    ProvidingData,
    Quitting,
}
