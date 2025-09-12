# SMTP2S 
## SMTP-to-Storage

A simple component that mimics an SMTP server but stores e-mail data instead of actually submitting it.

This was built considering usage in tests or to relay messages from systems that only implement SMTP when you need the integration to be done via other means.

### Features:
- Multiple message storage strategies:
    - S3
    - Local
- Basic ACL functionality,
- Structured logging formats.
- Metric exposure using OpenTelemetry.

### Use Cases

![Use cases diagram](__resources/use-cases.drawio.svg)

### What is stored

Every message is stored inside a folder with a dedicated execution ID (a simple ULID). Each execution folder will contain the following:

##### 🔎 Message Metadata (`metadata.json`)
```json
{
  "client": "[127.0.0.1]",
  "authenticated_user": "test@localhost.com",
  "from": "test@teste.com",
  "recipients": [
    "foo@bar.com"
  ],
  "to": [
    "foo@bar.com"
  ],
  "cc": [],
  "bcc": [],
  "subject": "teste",
  "date": "2025-09-11T22:43:34-03:00",
  "message_id": "6c2e0c6c-9535-4ae1-a920-3a6ffa036af5@teste.com"
}
```

##### ✉️ An HTML message body (`body.html`)

##### 📁 Attachments (in a dedicated attachment folder)

The file looks something like this:

```
storage-folder-or-s3-bucket/
├─ 01K4XSR779D9D9BTTVS7BBBRT3/
│  ├─ attachments/
│  |  ├─ file1.pdf
│  ├─ body.html
│  ├─ metadata.json
├─ 01K4XSRBY03MGFPQ3N6G0JW5ME/
│  ├─ attachments/
│  |  ├─ diagram-x.svg
│  |  ├─ flow-example.mp4
│  ├─ body.html
│  ├─ metadata.json
├─ 01K4XSRESDDW48DGZSN2KWB81D/
   ├─ attachments/
   ├─ body.html
   ├─ metadata.json
```

### Get Started

##### Running with local storage
```sh
cargo run -- --config-file=sample-configs/local-storage-config.json
```

##### Running with S3 based storage
```sh
docker compose up -d
cargo run -- --config-file=sample-configs/s3-config.json
```

##### Controlling Log Level using `--log-level`
```sh
# Define log level based on https://docs.rs/tracing/latest/tracing/struct.Level.html
cargo run -- --config-file={config-file} --log-level=DEBUG
```
> Default log level is `INFO`

##### Controlling logging format on an output-basis
```sh
cargo run -- --config-file={config-file} --stdout-log-kind=pretty --file-log-kind=json
```
> Default for `stdout` is `pretty`, and for file-based is `json`

##### Controlling file based log directory
```sh
cargo run -- --config-file={config-file} --file-log-dir=/logs/smtp2s
```
> Default is `logs`

#### `config-file` structure
```json
{
    // The port smtp2s will be server on
    "port": 8080,
    // Port to expose metrics, may be null, in that case metrics won't be exposed
    "metrics_port": 9090,
    // --- Strategies ---
    // S3 - Requires a bucket name and an optional override_aws_endpoint
    "strategy": {
        "type": "S3",
        "bucket_name": "smtp2s-data-storage",
        "override_aws_endpoint": "http://localhost:4566"
    },
    // Local - Requires a base path to store files
    "strategy": {
        "type": "Local",
        "base_path": "./local-storage"
    },
    // List of addresses allowed to submit e-mails, or "*" for any.
    "allowed_addresses": [
        "*"
    ]
}
```