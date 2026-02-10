# Unified Elastic Beanstalk Deployment (Tambourine + TURN)

This bundle deploys both services into a single Elastic Beanstalk Docker environment:

- `server` (Pipecat/Tambourine API)
- `coturn` (TURN relay)

## Important tradeoffs

- Use **single-instance** environments only.
- Server and TURN scale/restart together.
- A shared secret is required on both services.

## 1. Build and push both images

Push these images to ECR:

- `<AWS_ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com/tambourine-server:latest`
- `<AWS_ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com/coturn-turn-server:latest`

## 2. Update image URIs

Edit `docker-compose.yml` and replace:

- `<AWS_ACCOUNT_ID>`
- `<REGION>`

## 3. Package the EB config

```bash
cd elasticbeanstalk/unified
zip -r tambourine-unified-eb-config.zip docker-compose.yml .ebextensions/
```

## 4. Create Elastic Beanstalk environment

```bash
eb init tambourine-unified --platform docker --region <REGION>

# Single instance is required for UDP/WebRTC behavior
eb create tambourine-unified-prod \
  --single \
  --instance-type t3.medium \
  --envvars TURN_SHARED_SECRET=<your-secret>,TURN_CREDENTIAL_TTL=3600
```

## 5. Allocate and attach an Elastic IP

```bash
aws ec2 allocate-address --domain vpc

INSTANCE_ID=$(aws elasticbeanstalk describe-environment-resources \
  --environment-name tambourine-unified-prod \
  --query 'EnvironmentResources.Instances[0].Id' --output text)

aws ec2 associate-address --instance-id "$INSTANCE_ID" --allocation-id <AllocationId>
```

## 6. Configure runtime environment variables

Set all required Tambourine API keys plus TURN URL/secret:

```bash
eb setenv \
  TURN_SERVER_URL=turn:<ELASTIC_IP>:3478 \
  TURN_SHARED_SECRET=<same-secret-as-coturn> \
  TURN_CREDENTIAL_TTL=3600 \
  OPENAI_API_KEY=<...> \
  DEEPGRAM_API_KEY=<...>
```

`TURN_SERVER_URL` must be publicly reachable by clients (do not use `localhost`).

## 7. Verify

```bash
# Register client
CLIENT_UUID=$(curl -s -X POST "http://<ELASTIC_IP>:8765/api/client/register" | jq -r '.uuid')

# Fetch ICE config (requires registered UUID)
curl -s -H "X-Client-UUID: $CLIENT_UUID" "http://<ELASTIC_IP>:8765/api/ice-servers" | jq
```

You should see STUN and TURN entries in `ice_servers`.
