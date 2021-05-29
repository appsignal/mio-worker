# Mio Worker

Simple worker running on of [Mio](https://github.com/tokio-rs/mio) that can:

 * Receive messages
 * Schedule and run timeouts
 * Handle IO

Useful if you want to combine these three things in a loop that's
running in a single thread. Inspired on the design of Mio 0.5.

## Testing

It can be useful to view the `trace` level logs when running the tests:

```
RUST_LOG=trace cargo test -- --nocapture
```
