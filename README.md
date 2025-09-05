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