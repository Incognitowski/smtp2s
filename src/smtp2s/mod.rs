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
    // SAFETY: I think I can trim the input here. But we might have to refactor this when consuming buffers during `DATA` stage.
    let buffer_str = match std::str::from_utf8(buffer) {
        Ok(s) => s.trim(),
        Err(_) => return vec![b"500 Invalid UTF-8 sequence".to_vec()],
    };

    return match state {
        State::Initialized => initialize_trade(buffer_str, message_metadata, state),
        State::Authenticating { .. } => handle_auth_process(buffer_str, message_metadata, state),
        State::ProvidingHeaders => todo!(),
        State::ProvidingData => todo!(),
        State::Quitting => todo!(),
    };
}

fn initialize_trade(
    buffer_str: &str,
    message_metadata: &mut Metadata,
    state: &mut State,
) -> Vec<Vec<u8>> {
    let (command, client) = match buffer_str.split_once(' ') {
        Some((cmd, cl)) => (cmd, cl),
        None => return vec![b"501 Syntax error, expected: EHLO <domain>".to_vec()],
    };

    if !command.eq_ignore_ascii_case("EHLO") {
        return vec![b"552 Initial message must be EHLO".to_vec()];
    }
    message_metadata.client = client.trim().into();
    *state = State::Authenticating {
        state: AuthState::AwaithAuthRequest,
        username: None,
    };
    return vec![
        format!("250-smtp-proxy.mycompany.com greets {}", client)
            .as_bytes()
            .to_vec(),
        b"250-AUTH LOGIN PLAIN".to_vec(),
        b"250-SIZE 104857600".to_vec(),
        b"250 8BITMIME".to_vec(),
    ];
}

fn handle_auth_process(
    buffer_str: &str,
    message_metadata: &mut Metadata,
    state: &mut State,
) -> Vec<Vec<u8>> {
    let (auth_state, username) = if let State::Authenticating { state, username } = state {
        (state, username)
    } else {
        unreachable!("handle_auth_process called with a state other than AwaitingAuth");
    };

    return match auth_state {
        AuthState::AwaithAuthRequest => {
            if !buffer_str.eq_ignore_ascii_case("AUTH LOGIN") {
                vec![b"502 Command not implemented, expected AUTH LOGIN".to_vec()]
            } else {
                *auth_state = AuthState::RequestingUsername;
                vec![b"334 VXNlcm5hbWU6".to_vec()] // "Username:" in base64
            }
        }
        AuthState::RequestingUsername => {
            let decoded_username = match BASE64_STANDARD.decode(buffer_str) {
                Ok(bytes) => bytes,
                Err(_) => return vec![b"501 Syntax error in parameters (malformed base64)".to_vec()],
            };
            let parsed_username = match String::from_utf8(decoded_username) {
                Ok(s) => s,
                Err(_) => return vec![b"552 Invalid UTF-8 in username".to_vec()],
            };

            info!(?parsed_username, "Received username");
            message_metadata.authenticated_user = Some(parsed_username.clone());
            *username = Some(parsed_username);
            *auth_state = AuthState::RequestingPassword;
            vec![b"334 UGFzc3dvcmQ6".to_vec()] // "Password:" in base64
        }
        AuthState::RequestingPassword => {
            let decoded_password = match BASE64_STANDARD.decode(buffer_str) {
                Ok(bytes) => bytes,
                Err(_) => return vec![b"501 Syntax error in parameters (malformed base64)".to_vec()],
            };
            let parsed_password = match String::from_utf8(decoded_password) {
                Ok(s) => s,
                Err(_) => return vec![b"552 Invalid UTF-8 in password".to_vec()],
            };

            info!(?parsed_password, "Received password");
            // TODO: Actually validate user/password
            // Probably have to reset auth process if match fails
            *state = State::ProvidingHeaders;
            vec![b"235 2.7.0 Authentication successful".to_vec()]
        }
    };
}
