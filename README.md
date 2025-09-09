# SMTP2S 
## SMTP-to-Storage

A simple component that mimics an SMTP server but stores e-mail data instead of actually submitting it.

This was built considering usage in tests or to relay messages from systems that only implement SMTP when you need the integration to be done via other means.

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