# Accepts TCP connections and never answers: the "proxy that hangs" case.
import socket, sys, threading
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", int(sys.argv[1]))); s.listen(512)
conns = []
while True:
    c, _ = s.accept(); conns.append(c)
