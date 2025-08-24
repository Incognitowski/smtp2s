pub mod models;

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use models::AuthState;
use models::Metadata;
use models::State;
use tracing::info;

pub fn handle_message(
    buffer: &[u8],
    message_metadata: &mut Metadata,
    state: &mut State,
) -> Vec<Vec<u8>> {
    return match state {
        State::Initialized => initialize_trade(buffer, message_metadata, state),
        State::AwaitingAuth {
            state: _,
            username: _,
        } => handle_auth_process(buffer, message_metadata, state),
        State::Authenticated => todo!(),
        State::ProvidingHeaders => todo!(),
        State::ProvidingData => todo!(),
        State::Quitting => todo!(),
    };
}

fn initialize_trade(
    buffer: &[u8],
    message_metadata: &mut Metadata,
    state: &mut State,
) -> Vec<Vec<u8>> {
    let buffer_str =
        String::from_utf8(buffer.into()).expect("buffer to be a valid parseable string");
    let (command, client) = buffer_str
        .split_once(' ')
        .expect("for initial command to be formatted as 'EHLO client-name'");
    if command != "EHLO" {
        return vec![b"552 Initial message must be EHLO".to_vec()];
    }
    message_metadata.client = client.trim().into();
    *state = State::AwaitingAuth {
        state: AuthState::AwaithAuthRequest,
        username: None,
    };
    return vec![
        format!("250-smtp-proxy.mycompany.com greets {}", client.trim())
            .as_bytes()
            .to_vec(),
        b"250-AUTH LOGIN PLAIN".to_vec(),
        b"250-SIZE 104857600".to_vec(),
        b"250 8BITMIME".to_vec(),
    ];
}

fn handle_auth_process(
    buffer: &[u8],
    message_metadata: &mut Metadata,
    state: &mut State,
) -> Vec<Vec<u8>> {
    let (auth_state, username) = if let State::AwaitingAuth {
        state: auth_state,
        username,
    } = state
    {
        (auth_state, username)
    } else {
        return vec![b"552 Invalid state arrived at auth process".to_vec()];
    };

    return match auth_state {
        AuthState::AwaithAuthRequest => {
            let buffer_str =
                String::from_utf8(buffer.into()).expect("Buffer should be a parseable String");
            if buffer_str.trim() != "AUTH LOGIN" {
                vec![b"552 Expected auth request".to_vec()]
            } else {
                *auth_state = AuthState::RequestingUsername;
                vec![b"334 VXNlcm5hbWU6".to_vec()]
            }
        }
        AuthState::RequestingUsername => {
            let buffer_str =
                String::from_utf8(buffer.into()).expect("Buffer should be a parseable String");
            let parsed_username = match BASE64_STANDARD.decode(buffer_str.trim()) {
                Ok(value) => {
                    String::from_utf8(value).expect("Decoded username to be parseable as string")
                }
                Err(_) => return vec![b"552 Failed to decode username".to_vec()],
            };
            info!("Parsed username: {:?}", parsed_username);
            message_metadata.authenticated_user = Some(parsed_username.clone());
            *username = Some(parsed_username);
            *auth_state = AuthState::RequestingPassword;
            return vec![b"334 UGFzc3dvcmQ6".to_vec()];
        }
        AuthState::RequestingPassword => {
            let buffer_str =
                String::from_utf8(buffer.into()).expect("Buffer should be a parseable String");
            let parsed_password = match BASE64_STANDARD.decode(buffer_str.trim()) {
                Ok(value) => {
                    String::from_utf8(value).expect("Decoded password to be parseable as string")
                }
                Err(_) => return vec![b"552 Failed to decode password".to_vec()],
            };
            info!("Parsed password: {:?}", parsed_password);
            // TODO: Actually validate user/password
            // Probably have to reset auth process if match fails
            *state = State::Authenticated;
            return vec![b"235 Authentication successful".to_vec()];
        }
    };
}
