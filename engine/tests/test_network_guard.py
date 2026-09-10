"""A denied connection follows the socket API's failure/cleanup contract."""

import socket

import pytest

from conftest import NetworkAccessDenied


def test_denied_connection_closes_the_socket_it_created(monkeypatch):
    made = []
    original = socket.socket

    class TrackedSocket(original):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, **kwargs)
            made.append(self)

    monkeypatch.setattr(socket, "socket", TrackedSocket)
    try:
        with pytest.raises(NetworkAccessDenied):
            socket.create_connection(("203.0.113.1", 80), timeout=1)
        assert made, "the test never allocated a socket"
        assert all(sock.fileno() == -1 for sock in made), \
            "the denied connect bypassed the socket library's cleanup"
    finally:
        for sock in made:
            sock.close()
