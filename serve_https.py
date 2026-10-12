#!/usr/bin/env python3
import ssl
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

PORTA = 8443
CERT = "localhost+1.pem"
CHIAVE = "localhost+1-key.pem"


class Handler(SimpleHTTPRequestHandler):
    # il browser compila il WASM in streaming solo con questo tipo MIME
    extensions_map = {**SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm"}


server = ThreadingHTTPServer(("0.0.0.0", PORTA), Handler)

contesto = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
contesto.load_cert_chain(CERT, CHIAVE)
server.socket = contesto.wrap_socket(server.socket, server_side=True)

print(f"https://localhost:{PORTA}")
server.serve_forever()
