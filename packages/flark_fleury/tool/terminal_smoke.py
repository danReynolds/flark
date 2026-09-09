"""PTY launch/input/exit smoke, not an actual terminal emulator or IME proof."""
import fcntl
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import sys
import termios
import time

example = Path(__file__).resolve().parents[1] / 'example'
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 32, 100, 0, 0))
env = dict(os.environ, TERM='xterm-256color', COLORTERM='truecolor')
command = [sys.argv[1]] if len(sys.argv) > 1 else [str(example / 'build/cli/macos_arm64/bundle/bin/main')]
child = subprocess.Popen(command, cwd=example, stdin=slave, stdout=slave, stderr=slave, env=env)
os.close(slave)
output = bytearray()

def until(predicate, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if select.select([master], [], [], 0.05)[0]:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if not data:
                break
            output.extend(data)
        if predicate():
            return
    raise AssertionError('PTY condition timed out: ' + repr(output[-1600:]))

try:
    until(lambda: b'Changes stay in this session.' in output)
    os.write(master, b'\x01')  # Ctrl+A, real terminal key parser
    os.write(master, b'\x1b[200~**Terminal smoke**\n\nlast\x1b[201~')
    until(lambda: b'Terminal smoke' in output)
    os.write(master, b'!')
    until(lambda: b'!' in output)
    os.write(master, b'\x11')  # Ctrl+Q, application exit
    child.wait(timeout=10)
    assert child.returncode == 0, child.returncode
    print('PASS: native hooks, TTY launch, Ctrl+A, bracketed paste, typed input, clean exit')
finally:
    if child.poll() is None:
        child.terminate()
        child.wait(timeout=10)
    os.close(master)
