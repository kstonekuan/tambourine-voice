"""TURN server credential generation using HMAC-based time-limited authentication.

This module implements the credential scheme used by coturn's `use-auth-secret` mode.
Each client gets unique, short-lived credentials that can be validated by the TURN
server using a shared secret, without requiring the TURN server to maintain a user
database.

Protocol:
- username = expiry_timestamp (Unix time when credential expires)
- password = HMAC-SHA1(secret, username) base64-encoded

The TURN server validates credentials by:
1. Parsing the expiry timestamp from the username
2. Checking if current time < expiry (credential not expired)
3. Computing HMAC-SHA1(secret, username) and comparing to password

Reference: https://tools.ietf.org/html/draft-uberti-behave-turn-rest-00
"""

import base64
import hmac
import time
from dataclasses import dataclass


@dataclass
class TurnCredentials:
    """Time-limited TURN server credentials."""

    username: str
    password: str
    ttl: int


def generate_turn_credentials(secret: str, ttl: int = 3600) -> TurnCredentials:
    """Generate time-limited TURN credentials using HMAC-SHA1.

    Args:
        secret: The shared secret configured on both the TURN server
                and this application. Must match the TURN server's
                `static-auth-secret` configuration.
        ttl: Time-to-live in seconds. Credentials expire after this duration.
             Default is 3600 seconds (1 hour).

    Returns:
        TurnCredentials containing:
        - username: The expiry timestamp (Unix time)
        - password: HMAC-SHA1(secret, username) base64-encoded
        - ttl: The credential validity period in seconds

    Example:
        >>> creds = generate_turn_credentials("my-secret", ttl=3600)
        >>> print(f"username={creds.username}, password={creds.password}")
    """
    # Calculate expiry timestamp
    expiry_timestamp = int(time.time()) + ttl
    username = str(expiry_timestamp)

    # Generate HMAC-SHA1 of the username using the shared secret
    # This matches coturn's validation: hmac(secret, username)
    hmac_digest = hmac.new(
        key=secret.encode("utf-8"),
        msg=username.encode("utf-8"),
        digestmod="sha1",
    ).digest()

    # Base64 encode the HMAC digest for the password
    password = base64.b64encode(hmac_digest).decode("utf-8")

    return TurnCredentials(username=username, password=password, ttl=ttl)
