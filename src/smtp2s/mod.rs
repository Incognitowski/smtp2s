pub mod models;

use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::vec;

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::Utc;
use mail_parser::Address;
use mail_parser::MessageParser;
use mail_parser::MimeHeaders;
use models::AuthState;
use models::Metadata;
use models::State;
use tracing::info;
use twoway::find_str;
use twoway::rfind_bytes;
use ulid::Ulid;

use crate::smtp2s::models::HeadersState;

pub fn handle_message(
    buffer: &[u8],
    message_metadata: &mut Metadata,
    state: &mut State,
    data_vec: &mut Vec<u8>,
) -> Vec<Vec<u8>> {
    let buffer_str = match std::str::from_utf8(buffer) {
        Ok(s) => {
            // We cannot trim during DATA phase because we remove CRLFs, and they are necessary for the exchange.
            if matches!(state, State::ProvidingData) {
                s
            } else {
                s.trim()
            }
        }
        Err(_) => return vec![b"500 Invalid UTF-8 sequence".to_vec()],
    };

    info!(buffer_str, "Received command");

    return match state {
        State::Initialized => initialize_trade(buffer_str, message_metadata, state),
        State::Authenticating { .. } => handle_auth_process(buffer_str, message_metadata, state),
        State::ProvidingHeaders { .. } => handle_headers(buffer_str, message_metadata, state),
        State::ProvidingData => handle_data(buffer_str, message_metadata, state, data_vec),
        State::Quitting => handle_quit(buffer_str),
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
            let decoded_password = match BASE64_STANDARD.decode(buffer_str) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return vec![b"501 Syntax error in parameters (malformed base64)".to_vec()];
                }
            };
            let parsed_password = match String::from_utf8(decoded_password) {
                Ok(s) => s,
                Err(_) => return vec![b"552 Invalid UTF-8 in password".to_vec()],
            };

            info!(?parsed_password, "Received password");
            // TODO: Actually validate user/password
            // Probably have to reset auth process if match fails
            *state = State::ProvidingHeaders {
                state: HeadersState::ProvidingFrom,
            };
            vec![b"235 2.7.0 Authentication successful".to_vec()]
        }
    };
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

    return match headers_state {
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
            return vec![b"250 OK".to_vec()];
        }
        HeadersState::ProvidingRecipients => {
            if buffer_str == "DATA" {
                if message_metadata.recipients.is_empty() {
                    // TODO: Is 501 the proper code here?
                    return vec![
                        b"501 Client must provide at least one recipient before calling DATA"
                            .to_vec(),
                    ];
                }
                *state = State::ProvidingData;
                // Do I need to actually return <CRLF> or does it have to be \r\n?
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
    };
}

fn handle_data(
    buffer_str: &str,
    message_metadata: &mut Metadata,
    state: &mut State,
    data_vec: &mut Vec<u8>,
) -> Vec<Vec<u8>> {
    data_vec.extend_from_slice(buffer_str.as_bytes());

    return match rfind_bytes(&data_vec, DATA_TERMINATOR.as_bytes()) {
        Some(idx) => {
            let relevant_buffer_section = std::str::from_utf8(&data_vec[0..idx]).unwrap();
            let relevant_buffer_str = sanitize_dot_stuffing(relevant_buffer_section);
            let message = match MessageParser::default().parse(&relevant_buffer_str) {
                Some(message) => message,
                None => return vec![b"501 Syntax Error, could not parse provided data.".to_vec()],
            };
            // info!("{:?}", message);
            message_metadata.to = address_to_vec(&message.to());
            message_metadata.cc = address_to_vec(&message.cc());
            message_metadata.bcc = address_to_vec(&message.bcc());
            message_metadata.subject = match message.subject() {
                Some(sbj) => sbj.to_string(),
                None => "No Subject".to_string(),
            };
            message_metadata.date = match message.date() {
                Some(date) => Some(date.to_rfc3339()),
                None => Some(Utc::now().to_rfc3339()),
            };
            message_metadata.message_id = match message.message_id() {
                Some(message_id) => Some(message_id.to_string()),
                None => None,
            };

            // We'll .unwrap for now but must consider making this prettier
            let execution = Ulid::new().to_string();
            let base_folder = "/home/incognitowski/Desktop/tmp-storage";
            let mut metadata_output_file =
                File::create_new(format!("{base_folder}/{}-metadata.json", &execution)).unwrap();
            let mut body_output_file =
                File::create_new(format!("{base_folder}/{}-body.html", &execution)).unwrap();

            metadata_output_file
                .write_all(serde_json::to_string(&message_metadata).unwrap().as_bytes())
                .unwrap();
            body_output_file
                .write_all(
                    message
                        .body_html(0)
                        .map(|cow| cow.to_string())
                        .unwrap()
                        .as_bytes(),
                )
                .unwrap();
            
            save_attachments_from_message(&message, 0);

            *state = State::Quitting;
            return vec![b"250 Message accepted for delivery".to_vec()];
        }
        None => vec![],
    };
}

const DATA_TERMINATOR: &str = "\r\n.\r\n";

fn handle_quit(buffer_str: &str) -> Vec<Vec<u8>> {
    return if buffer_str.eq_ignore_ascii_case("QUIT") {
        vec![b"221 Bye".to_vec()]
    } else {
        vec![b"501 Expected QUIT.".to_vec()]
    };
}

fn sanitize_dot_stuffing(raw_str: &str) -> String {
    return raw_str.replace("..", ".");
}

fn sanitize_address(address: &str) -> &str {
    let trimmed_address = address.trim();
    let first_cut_idx = find_str(trimmed_address, "<").unwrap_or(0) + 1;
    let last_cust_idx = find_str(trimmed_address, ">").unwrap_or(trimmed_address.len());
    return &trimmed_address[first_cut_idx..last_cust_idx]
}

fn address_to_vec(address: &Option<&Address>) -> Vec<String> {
    return match address {
        Some(addresses) => match addresses.as_list() {
            Some(recipients) => recipients
                .iter()
                // This looks awful! :(
                .filter_map(|addr| addr.address.as_ref().map(|cow| cow.to_string()))
                .collect(),
            None => vec![],
        },
        None => vec![],
    };
}

fn save_attachments_from_message(
    msg: &mail_parser::Message,
    depth: usize,
) {
    let out_dir = Path::new("/home/incognitowski/Desktop/tmp-storage/");
    fs::create_dir_all(out_dir).unwrap();

    for (i, part) in msg.attachments().enumerate() {
        let mut name = part.attachment_name()
            .or_else(|| part.content_disposition().and_then(|cd| cd.attribute("filename")))
            .or_else(|| part.content_type().and_then(|ct| ct.attribute("name")))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("attachment-{}-{}", depth, i + 1));

        name = sanitize_filename::sanitize(&name);

        if std::path::Path::new(&name).extension().is_none() {
            if let Some(ct) = part.content_type() {
                let mime = format!("{}/{}", ct.ctype(), ct.subtype().unwrap_or("octet-stream"));
                if let Some(exts) = mime_guess::get_mime_extensions_str(&mime) {
                    name.push('.');
                    name.push_str(exts[0]);
                }
            }
        }

        let path = dedupe_filename(out_dir.join(&name));
        fs::write(&path, part.contents()).unwrap();

        if let Some(nested) = part.message() {
            save_attachments_from_message(nested, depth + 1);
        }
    }
}

fn dedupe_filename(path: PathBuf) -> PathBuf {
    if !path.exists() { return path; }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("attachment");
    let ext  = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for n in 2.. {
        let candidate = if ext.is_empty() {
            path.with_file_name(format!("{stem} ({n})"))
        } else {
            path.with_file_name(format!("{stem} ({n}).{ext}"))
        };
        if !candidate.exists() { return candidate; }
    }
    unreachable!()
}