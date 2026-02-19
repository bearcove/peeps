# cf. https://github.com/casey/just

list:
    just --list

dev:
    cargo run --bin peeps-web -- --dev

example *args:
    cargo run --bin peeps-examples -- {{ args }}

ex *args:
    just kill-port # fuck you too, vite
    cargo run --bin peeps-examples -- {{ args }}

kill-port port="9132":
    lsof -ti:{{ port }} -sTCP:LISTEN | xargs kill -9
