# totp.py — TOTP code generator for IRC authentication
#
# Demonstrates full import access — uses pyotp if installed.
# Install with: pip install pyotp
#
# Usage:
#   /totp <secret>     — Generate a TOTP code from a secret
#   /totp              — Generate from saved secret (via /totp set <secret>)

import flume

try:
    import pyotp
    HAS_PYOTP = True
except ImportError:
    HAS_PYOTP = False

def handle_totp(args):
    if not HAS_PYOTP:
        flume.buffer.print("", "", "pyotp not installed. Run: pip install pyotp")
        return

    parts = args.strip().split(None, 1)
    if parts and parts[0] == "set":
        if len(parts) < 2:
            flume.buffer.print("", "", "Usage: /totp set <secret>")
            return
        flume.config.set("secret", parts[1])
        flume.buffer.print("", "", "TOTP secret saved")
        return

    secret = args.strip() if args.strip() else flume.config.get("secret")
    if not secret:
        flume.buffer.print("", "", "Usage: /totp <secret> or /totp set <secret> first")
        return

    try:
        totp = pyotp.TOTP(secret)
        code = totp.now()
        flume.buffer.print("", "", f"TOTP code: {code}")
    except Exception as e:
        flume.buffer.print("", "", f"TOTP error: {e}")

flume.command.register("totp", handle_totp, "Generate TOTP code")
