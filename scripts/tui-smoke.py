#!/usr/bin/env python3
"""Drive the TUI through a pty and print stripped frames. Usage: scripts/tui-smoke.py [binary]"""
import os, pty, re, select, signal, struct, sys, termios, fcntl, time

binary = sys.argv[1] if len(sys.argv) > 1 else "target/debug/ankh"
pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execv(binary, ["ankh"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 110, 0, 0))
os.kill(pid, signal.SIGWINCH)
buf = b""

def pump(t):
    global buf
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.1)
        if r:
            try:
                buf += os.read(fd, 65536)
            except OSError:
                return

def snap(label):
    global buf
    txt = re.sub(rb"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][A-Z0-9]|\x1b[=>]", b"", buf).decode("utf-8", "replace")
    print(f"===== {label}")
    print("\n".join(l.rstrip() for l in txt.splitlines() if l.strip())[-3000:])
    buf = b""

pump(4); snap("launch")
for k in b"jjjl ":
    os.write(fd, bytes([k])); pump(0.3)
snap("moved + which-key")
os.write(fd, b"\x1b?"); pump(0.5); snap("help")
os.write(fd, b"?ZQ"); pump(1)
print("exit", os.waitpid(pid, 0)[1])
