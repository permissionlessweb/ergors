#!/usr/bin/env python3
"""
generate_keys.py
----------------
Generate deterministic Ed25519 and secp256k1 key pairs from user‑provided entropy.

Usage:
    python generate_keys.py <entropy_file_or_hex_string>
"""

import sys
import os
import binascii
import base64
from typing import Tuple, Union

# ----------------------------------------------------------------------
# Crypto imports
# ----------------------------------------------------------------------
from cryptography.hazmat.primitives.asymmetric import ed25519, ec
from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
# No backend import needed for recent cryptography versions
import ecdsa  # pure‑python ECDSA, supports SECP256k1

# ----------------------------------------------------------------------
# Helper functions
# ----------------------------------------------------------------------


def _to_hex(data: Union[bytes, str]) -> str:
    """Canonical hex string (lower‑case, no 0x, no whitespace)."""
    if isinstance(data, bytes):
        raw = data
    else:  # str → UTF‑8 bytes
        raw = data.encode("utf-8")
    return binascii.hexlify(raw).decode("ascii")


def read_entropy(source: str) -> bytes:
    """
    Load entropy from *source* and always return raw bytes.

    * file path → raw file contents
    * hex string (with/without 0x) → decoded bytes
    * anything else → treat as UTF‑8 text, hex‑encode, then decode
    """
    # ---- 1️⃣  File ----------------------------------------------------
    if os.path.isfile(source):
        with open(source, "rb") as f:
            data = f.read()
        if not data:
            raise ValueError("Entropy file is empty.")
        # Run through the canonical hex path for consistency
        return binascii.unhexlify(_to_hex(data))

    # ---- 2️⃣  Hex string -----------------------------------------------
    cleaned = source.strip().replace("0x", "").replace(" ", "")
    try:
        return binascii.unhexlify(cleaned)
    except (binascii.Error, ValueError):
        pass  # not a valid hex → fall through

    # ---- 3️⃣  Plain‑text fallback ---------------------------------------
    # Encode as UTF‑8 → hex → bytes
    return binascii.unhexlify(_to_hex(source))


def hkdf_derive_seed(
    entropy: bytes,
    length: int = 32,
    salt: bytes = b"gen_keys_salt",
    info: bytes = b"ed25519+secp256k1",
) -> bytes:
    """
    Derive a uniformly random ``length``‑byte seed from arbitrary ``entropy``.
    """
    hkdf = HKDF(
        algorithm=hashes.SHA256(),   # ← **cryptography** hash object
        length=length,
        salt=salt,
        info=info,
        # backend=default_backend(),   # ← removed for modern cryptography
    )
    return hkdf.derive(entropy)


# ----------------------------------------------------------------------
# Ed25519
# ----------------------------------------------------------------------


def gen_ed25519(seed: bytes) -> Tuple[bytes, bytes]:
    """Return (private_raw, public_raw) for Ed25519 from a 32‑byte seed."""
    private_key = ed25519.Ed25519PrivateKey.from_private_bytes(seed)
    public_key = private_key.public_key()

    priv_raw = private_key.private_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PrivateFormat.Raw,
        encryption_algorithm=serialization.NoEncryption(),
    )
    pub_raw = public_key.public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw,
    )
    return priv_raw, pub_raw


def ed25519_pem(private_raw: bytes) -> bytes:
    """PEM (PKCS#8) representation of an Ed25519 private key."""
    private_key = ed25519.Ed25519PrivateKey.from_private_bytes(private_raw)
    return private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption(),
    )


# ----------------------------------------------------------------------
# secp256k1 (ECDSA)
# ----------------------------------------------------------------------


def gen_secp256k1(seed: bytes) -> Tuple[bytes, bytes]:
    """
    Return (priv_raw, pub_raw) for secp256k1.
    * priv_raw – 32‑byte scalar (big‑endian)
    * pub_raw  – 33‑byte compressed SEC format
    """
    sk = ecdsa.SigningKey.from_string(seed, curve=ecdsa.SECP256k1)
    vk = sk.get_verifying_key()

    priv_raw = sk.to_string()                     # 32‑byte scalar
    pub_raw = vk.to_string("compressed")         # 33‑byte SEC‑compressed
    return priv_raw, pub_raw


def secp256k1_pem(private_raw: bytes) -> bytes:
    """PEM (PKCS#8) representation of a secp256k1 private key."""
    # Build a cryptography EC private key from the scalar:
    private_numbers = ec.EllipticCurvePrivateNumbers(
        private_value=int.from_bytes(private_raw, "big"),
        public_numbers=ec.EllipticCurvePublicNumbers.from_encoded_point(
            ec.SECP256K1(),
            # The same compressed point we derived with *ecdsa*
            private_raw * 0  # dummy – will be replaced below
        ),
    )
    # Replace the dummy point with the real one:
    signing_key = ecdsa.SigningKey.from_string(private_raw, curve=ecdsa.SECP256k1)
    compressed = signing_key.get_verifying_key().to_string("compressed")
    private_numbers.public_numbers = ec.EllipticCurvePublicNumbers.from_encoded_point(
        ec.SECP256K1(), compressed
    )
    private_key = private_numbers.private_key()

    return private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption(),
    )


# ----------------------------------------------------------------------
# Pretty‑printing helpers
# ----------------------------------------------------------------------


def b64(b: bytes) -> str:
    return base64.b64encode(b).decode("ascii")


def print_keyset(name: str, priv: bytes, pub: bytes, priv_pem: bytes):
    print(f"\n=== {name} ===")
    print(f"Private (hex)   : {priv.hex()}")
    print(f"Public  (hex)   : {pub.hex()}")
    print(f"Private (base64): {b64(priv)}")
    print(f"Public  (base64): {b64(pub)}")
    print("\n--- PEM ---------------------------------------------------")
    print(priv_pem.decode().strip())
    print("-----------------------------------------------------------")


# ----------------------------------------------------------------------
# Main driver
# ----------------------------------------------------------------------


def main() -> None:
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(1)

    try:
        entropy = read_entropy(sys.argv[1])
    except Exception as exc:
        print(f"Error reading entropy: {exc}", file=sys.stderr)
        sys.exit(2)

    seed = hkdf_derive_seed(entropy)          # 32‑byte uniform seed

    # ---- Ed25519 ----------------------------------------------------
    ed_priv, ed_pub = gen_ed25519(seed)
    ed_pem = ed25519_pem(ed_priv)
    print_keyset("Ed25519", ed_priv, ed_pub, ed_pem)

    # ---- secp256k1 --------------------------------------------------
    secp_priv, secp_pub = gen_secp256k1(seed)
    secp_pem = secp256k1_pem(secp_priv)
    print_keyset("secp256k1 (ECDSA)", secp_priv, secp_pub, secp_pem)


if __name__ == "__main__":
    main()