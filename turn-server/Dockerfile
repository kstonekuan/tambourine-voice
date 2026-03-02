# Coturn TURN server for Tambourine WebRTC NAT traversal
# Uses HMAC-based time-limited credentials for security

FROM coturn/coturn:4-alpine

# Copy static configuration and entrypoint
COPY turnserver.conf /etc/turnserver.conf
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
USER root
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
USER nobody

# Expose TURN ports
# 3478: TURN (UDP/TCP)
# 5349: TURNS (TLS)
# 49152-65535: Media relay ports
EXPOSE 3478/udp 3478/tcp 5349/tcp 49152-65535/udp

# Run coturn with config file and runtime-injected shared secret
# -n: do not daemonize
CMD ["/usr/local/bin/docker-entrypoint.sh"]
