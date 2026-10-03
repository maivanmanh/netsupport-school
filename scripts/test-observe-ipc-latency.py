#!/usr/bin/env python3
"""Probe mouse-rate IPC dispatch without injecting input into any Student.

Authenticates to an existing Teacher endpoint; retains only aggregate timing.
This measures command dispatch, not network/video or glass-to-glass latency.
"""
import argparse
import json
from pathlib import Path
import socket
import struct
import threading
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('endpoint', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--count', type=int, default=300)
    parser.add_argument('--rate', type=float, default=125)
    parser.add_argument('--max-p95-ms', type=float, default=25)
    args = parser.parse_args()
    assert 1 <= args.count <= 2000 and 1 <= args.rate <= 250
    endpoint = json.loads(args.endpoint.read_text())
    with socket.create_connection(('127.0.0.1', endpoint['port']), timeout=15) as sock:
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

        def send(value):
            data = json.dumps(value).encode()
            sock.sendall(struct.pack('!I', len(data)) + data)

        def exact(length):
            data = b''
            while len(data) < length:
                part = sock.recv(length - len(data))
                if not part:
                    raise EOFError('Teacher IPC closed')
                data += part
            return data

        def receive():
            length = struct.unpack('!I', exact(4))[0]
            assert 0 < length <= 8 * 1024 * 1024
            return json.loads(exact(length))

        send({'type': 'hello', 'protocol': 1, 'token': endpoint['token']})
        receive()
        sent, samples, failures = {}, [], []

        def write():
            try:
                began = time.monotonic()
                for index in range(1, args.count + 1):
                    time.sleep(max(0, began + (index - 1) / args.rate - time.monotonic()))
                    sent[index] = time.monotonic()
                    send({'id': index, 'action': 'observe_input',
                          'selected': ['latency-probe-no-session'],
                          'input': {'action': 'move', 'x': .5, 'y': .5}})
            except Exception as error:
                failures.append(str(error))

        writer = threading.Thread(target=write, daemon=True)
        writer.start()
        while len(samples) < args.count:
            value = receive()
            index = value.get('id', 0)
            if value.get('type') == 'ack' and index in sent:
                samples.append((time.monotonic() - sent[index]) * 1000)
        writer.join(timeout=1)
        assert not failures, failures
    samples.sort()
    result = {'count': args.count, 'rate_hz': args.rate,
              'p50_ms': samples[len(samples) // 2],
              'p95_ms': samples[int((len(samples) - 1) * .95)],
              'max_ms': max(samples),
              'note': 'Local IPC dispatch only; no remote input or video'}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
    if result['p95_ms'] > args.max_p95_ms:
        raise SystemExit('IPC cannot sustain the configured input rate')


if __name__ == '__main__':
    main()
