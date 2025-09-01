use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use mail_parser::{Address, MessageParser};
use tracing::{error, info};
use twoway::{find_str, rfind_bytes};

use crate::smtp::models::{AuthState, HeadersState, Metadata, State};
use crate::storage::Storage;

pub fn handle_message<T: Storage>(
    buffer: &[u8],
    message_metadata: &mut Metadata,
    state: &mut State,
    data_vec: &mut Vec<u8>,
    storage: &T,
) -> Vec<Vec<u8>> {
    let buffer_str = match std::str::from_utf8(buffer) {
        Ok(s) => {
            if matches!(state, State::ProvidingData) {
                s
            } else {
                s.trim()
            }
        }
        Err(_) => return vec![b"500 Invalid UTF-8 sequence".to_vec()],
    };

    info!(buffer_str, "Received command");

    match state {
        State::Initialized => initialize_trade(buffer_str, message_metadata, state),
        State::Authenticating { .. } => handle_auth_process(buffer_str, message_metadata, state),
        State::ProvidingHeaders { .. } => handle_headers(buffer_str, message_metadata, state),
        State::ProvidingData => {
            handle_data(buffer_str, message_metadata, state, data_vec, storage)
        }
        State::Quitting => handle_quit(buffer_str),
    }
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
    vec![
        format!("250-smtp-proxy.mycompany.com greets {}", client)
            .as_bytes()
            .to_vec(),
        b"250-AUTH LOGIN PLAIN".to_vec(),
        b"250-SIZE 104857600".to_vec(),
        b"250 8BITMIME".to_vec(),
    ]
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

    match auth_state {
        AuthState::AwaithAuthRequest => {
            if !buffer_str.eq_ignore_ascii_case("AUTH LOGIN") {
                vec![b"530 5.7.0 Authentication required".to_vec()]
            } else {
                *auth_state = AuthState::RequestingUsername;
                vec![b"334 VXNlcm5hbWU6".to_vec()] // "Username:" in base64
            }
        }
        AuthState::RequestingUsername => {
            let decoded_username = match BASE64_STANDARD.decode(buffer_str) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return vec![b"501 Syntax error in parameters (malformed base64)".to_vec()];
                }
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
            // TODO: Actually validate user/password against provided file
            *state = State::ProvidingHeaders {
                state: HeadersState::ProvidingFrom,
            };
            vec![b"235 2.7.0 Authentication successful".to_vec()]
        }
    }
}

fn handle_headers(
    buffer_str: &str,
    message_metadata: &mut Metadata,
    state: &mut State,
) -> Vec<Vec<u8>> {
    let headers_state = if let State::ProvidingHeaders { state } = state {
        state
    } else {
        unreachable!("handle_headers called with a state other than ProvidingHeaders");
    };

    match headers_state {
        HeadersState::ProvidingFrom => {
            let (command, mail_from) = match buffer_str.split_once(':') {
                Some((cmd, mail_from)) => (cmd, sanitize_address(mail_from)),
                None => return vec![b"501 Syntax error, expected: 'MAIL FROM:<address>'".to_vec()],
            };
            if !command.eq_ignore_ascii_case("MAIL FROM") {
                return vec![b"501 Syntax error, expected: 'MAIL FROM:<address>'".to_vec()];
            }
            message_metadata.from = mail_from.into();
            *headers_state = HeadersState::ProvidingRecipients;
            vec![b"250 OK".to_vec()]
        }
        HeadersState::ProvidingRecipients => {
            if buffer_str.eq_ignore_ascii_case("DATA") {
                if message_metadata.recipients.is_empty() {
                    return vec![
                        b"503 Client must provide at least one recipient before calling DATA"
                            .to_vec(),
                    ];
                }
                *state = State::ProvidingData;
                return vec![b"354 End data with <CRLF>.<CRLF>".to_vec()];
            }
            let (command, mail_to) = match buffer_str.split_once(':') {
                Some((cmd, mail_to)) => (cmd, sanitize_address(mail_to)),
                None => return vec![b"501 Syntax error, expected: 'RCPT TO:<address>'".to_vec()],
            };
            if !command.eq_ignore_ascii_case("RCPT TO") {
                return vec![b"501 Syntax error, expected: 'RCPT TO:<address>'".to_vec()];
            }
            if !message_metadata.recipients.contains(&mail_to.to_string()) {
                message_metadata.recipients.push(mail_to.into());
            }
            vec![b"250 OK".to_vec()]
        }
    }
}

const DATA_TERMINATOR: &str = "\r\n.\r\n";

fn handle_data<T: Storage>(
    buffer_str: &str,
    message_metadata: &mut Metadata,
    state: &mut State,
    data_vec: &mut Vec<u8>,
    storage: &T,
) -> Vec<Vec<u8>> {
    data_vec.extend_from_slice(buffer_str.as_bytes());

    if rfind_bytes(&data_vec, DATA_TERMINATOR.as_bytes()).is_some() {
        let relevant_buffer_str = sanitize_dot_stuffing(std::str::from_utf8(&data_vec).unwrap());
        let message = match MessageParser::default().parse(&relevant_buffer_str) {
            Some(message) => message,
            None => return vec![b"501 Syntax Error, could not parse provided data.".to_vec()],
        };

        message_metadata.to = address_to_vec(&message.to());
        message_metadata.cc = address_to_vec(&message.cc());
        message_metadata.bcc = address_to_vec(&message.bcc());
        message_metadata.subject = message.subject().map(String::from).unwrap_or_default();
        message_metadata.date = message.date().map(|d| d.to_rfc3339());
        message_metadata.message_id = message.message_id().map(String::from);

        if let Err(e) = storage.save(message_metadata, &message) {
            error!(error.message = %e, "Failed to save message");
            return vec![b"554 Transaction failed".to_vec()];
        }

        *state = State::Quitting;
        return vec![b"250 Message accepted for delivery".to_vec()];
    }
    vec![]
}

fn handle_quit(buffer_str: &str) -> Vec<Vec<u8>> {
    if buffer_str.eq_ignore_ascii_case("QUIT") {
        vec![b"221 Bye".to_vec()]
    } else {
        vec![b"501 Expected QUIT.".to_vec()]
    }
}

fn sanitize_dot_stuffing(raw_str: &str) -> String {
    raw_str
        .strip_suffix(DATA_TERMINATOR)
        .unwrap_or(raw_str)
        .replace("..", ".")
}

fn sanitize_address(address: &str) -> &str {
    let trimmed_address = address.trim();
    let first_cut_idx = find_str(trimmed_address, "<").unwrap_or(0) + 1;
    let last_cust_idx = find_str(trimmed_address, ">").unwrap_or(trimmed_address.len());
    &trimmed_address[first_cut_idx..last_cust_idx]
}

fn address_to_vec(address: &Option<&Address>) -> Vec<String> {
    match address {
        Some(addresses) => match addresses.as_list() {
            Some(recipients) => recipients
                .iter()
                .filter_map(|addr| addr.address.as_ref().map(|cow| cow.to_string()))
                .collect(),
            None => vec![],
        },
        None => vec![],
    }
}
