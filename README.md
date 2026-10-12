# Q-learning nel browser (Slint + WebAssembly)

## Compilare per il web

Una sola volta:

    rustup target add wasm32-unknown-unknown
    cargo install wasm-pack

Poi, nella cartella del progetto:

    wasm-pack build --release --target web

Avviare un server locale (serve, il file:// non funziona con i moduli WASM):

    python3 -m http.server 8080

Aprire http://localhost:8080 nel browser.

## Alternativa senza wasm-pack

    cargo build --release --target wasm32-unknown-unknown
    cargo install wasm-bindgen-cli
    wasm-bindgen --target web --out-dir pkg --out-name qlearning_web target/wasm32-unknown-unknown/release/qlearning_web.wasm

## Versione desktop (stessa interfaccia)

    cargo run --release --bin qlearning_desktop
