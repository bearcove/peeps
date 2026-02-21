# cf. https://github.com/casey/just

list:
    just --list

dev:
    cargo run --bin moire-web -- --dev

example *args:
    cargo run --bin moire-examples -- {{ args }}

ex-prep:
    rm *sqlite* || true
    just kill-port # meh

ex *args: ex-prep
    RUST_LOG=debug cargo run --bin moire-examples -- {{ args }}

exr *args: ex-prep
    RUST_LOG=debug cargo run --features roam --bin moire-examples -- {{ args }}

lint:
    pnpm lint

kill-port port="9132":
    lsof -ti:{{ port }} -sTCP:LISTEN | xargs kill -9
