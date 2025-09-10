use super::*;
use crate::smtp::models::{Metadata, State};
use crate::storage::Storage;
use async_trait::async_trait;
use mail_parser::Message;

// A mock storage implementation that does nothing, for testing the protocol.
struct MockStorage;

#[async_trait]
impl Storage for MockStorage {
    async fn save(&self, _metadata: &Metadata, _message: &Message<'_>) -> Result<(), std::io::Error> {
        Ok(())
    }
}

macro_rules! assert_response {
    ($response:expr, $expected:expr) => {
        let response_str = String::from_utf8($response[0].clone()).unwrap();
        assert!(response_str.starts_with($expected));
    };
}

#[tokio::test]
async fn test_smtp_protocol_flow() {
    let mut message_metadata = Metadata::default();
    let mut state = State::Initialized;
    let mut data_vec: Vec<u8> = vec![];
    let storage = MockStorage {};
    let allowed_addresses = vec!["test@example.com".to_string()];

    // 1. Initialize transaction
    let response = handle_message(
        b"EHLO test.client\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "250-");
    assert!(matches!(state, State::Authenticating { .. }));

    // 2. Start authentication
    let response = handle_message(
        b"AUTH LOGIN\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "334"); // "Username:"

    // 3. Provide username
    let response = handle_message(
        b"dGVzdEBleGFtcGxlLmNvbQ==\r\n", // test@example.com
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "334"); // "Password:"

    // 4. Provide password
    let response = handle_message(
        b"cGFzc3dvcmQ=\r\n", // password
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "235");
    assert!(matches!(state, State::ProvidingHeaders { .. }));

    // 5. Provide e-mail sender
    let response = handle_message(
        b"MAIL FROM:<sender@example.com>\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "250");

    // 6. Provide recipient
    let response = handle_message(
        b"RCPT TO:<recipient@example.com>\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "250");

    // 7. Initialize DATA provide phase
    let response = handle_message(
        b"DATA\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "354");
    assert!(matches!(state, State::ProvidingData));

    // 8. Inform mail content
    let email_data = "From: <sender@example.com>\r\nTo: <recipient@example.com>\r\nSubject: Test\r\n\r\nBody\r\n.\r\n";
    let response = handle_message(
        email_data.as_bytes(),
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "250");
    assert!(matches!(state, State::Quitting));

    // 9. Finish
    let response = handle_message(
        b"QUIT\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;
    assert_response!(response, "221");
}

#[tokio::test]
async fn test_invalid_authorization() {
    let mut message_metadata = Metadata::default();
    let mut state = State::Authenticating {
        state: crate::smtp::models::AuthState::RequestingUsername,
        username: None,
    };
    let mut data_vec: Vec<u8> = vec![];
    let storage = MockStorage {};
    let allowed_addresses = vec!["valid@example.com".to_string()];

    let response = handle_message(
        b"d3JvbmcudXNlckBleGFtcGxlLmNvbQ==\r\n", // wrong.user@example.com
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;

    assert_response!(response, "535");
}

#[tokio::test]
async fn test_initial_message_not_ehlo() {
    let mut message_metadata = Metadata::default();
    let mut state = State::Initialized;
    let mut data_vec: Vec<u8> = vec![];
    let storage = MockStorage {};
    let allowed_addresses = vec!["*".to_string()];

    let response = handle_message(
        b"MAIL FROM:<sender@example.com>\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;

    assert_response!(response, "552");
}

#[tokio::test]
async fn test_data_before_recipient() {
    let mut message_metadata = Metadata::default();
    let mut state = State::ProvidingHeaders {
        state: crate::smtp::models::HeadersState::ProvidingRecipients,
    };
    let mut data_vec: Vec<u8> = vec![];
    let storage = MockStorage {};
    let allowed_addresses = vec!["*".to_string()];

    // Set state to after MAIL FROM has been successfully called
    message_metadata.from = "sender@example.com".to_string();

    let response = handle_message(
        b"DATA\r\n",
        &mut message_metadata,
        &mut state,
        &mut data_vec,
        &storage,
        &allowed_addresses,
    )
    .await;

    assert_response!(response, "503");
}
