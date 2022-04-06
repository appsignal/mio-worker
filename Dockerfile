FROM rust:1.59

WORKDIR /Users/luismiguelramirezmela/code/mio-worker

COPY . .

RUN RUST_LOG=trace cargo test --test io -- --nocapture

CMD tail -f /dev/null
