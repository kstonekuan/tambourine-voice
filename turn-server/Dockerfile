# Coturn TURN server for Tambourine WebRTC NAT traversal
# Uses HMAC-based time-limited credentials for security

FROM coturn/coturn:4-alpine

# Copy custom configuration
COPY turnserver.conf /etc/turnserver.conf

# Expose TURN ports
# 3478: TURN (UDP/TCP)
# 5349: TURNS (TLS)
# 49152-65535: Media relay ports
EXPOSE 3478/udp 3478/tcp 5349/tcp 49152-65535/udp

# Run coturn with config file
# -n: do not daemonize
# -v: verbose (optional, for debugging)
CMD ["turnserver", "-c", "/etc/turnserver.conf", "-n"]
