# TURN Server Deployment Guide

Deploy coturn TURN server to AWS Elastic Beanstalk for Tambourine WebRTC NAT traversal.

This Elastic Beanstalk setup uses Docker Compose with `network_mode: host` so coturn relay UDP ports (`49152-65535`) are reachable for media.

## Prerequisites

- AWS CLI configured with appropriate permissions
- Docker installed locally
- EB CLI installed (`pip install awsebcli`)

## 1. Generate Shared Secret

Generate a secure shared secret (32+ random characters):

```bash
openssl rand -hex 32
```

Save this secret - you'll need it for both the TURN server and Tambourine server.

## 2. Build and Push Docker Image

### Create ECR Repository

```bash
aws ecr create-repository --repository-name coturn-turn-server
```

### Build and Push

```bash
# Get login token
aws ecr get-login-password --region <REGION> | docker login --username AWS --password-stdin <AWS_ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com

# Build from turn-server directory
cd turn-server
docker build -t coturn-turn-server .

# Tag and push
docker tag coturn-turn-server:latest <AWS_ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com/coturn-turn-server:latest
docker push <AWS_ACCOUNT_ID>.dkr.ecr.<REGION>.amazonaws.com/coturn-turn-server:latest
```

## 3. Update `docker-compose.yml`

Edit `elasticbeanstalk/turn/docker-compose.yml` and replace:
- `<AWS_ACCOUNT_ID>` with your AWS account ID
- `<REGION>` with your region (e.g., `us-east-1`)

## 4. Package Elastic Beanstalk Config

```bash
cd elasticbeanstalk/turn
zip -r coturn-eb-config.zip docker-compose.yml .ebextensions/
```

## 5. Create Elastic Beanstalk Application

```bash
# Initialize EB application
eb init coturn-turn-server --platform docker --region <REGION>

# Create environment (single-instance to avoid load balancer complications with UDP)
eb create coturn-prod \
  --single \
  --instance-type t3.micro \
  --envvars TURN_SHARED_SECRET=<your-secret-here>
```

## 6. Allocate Elastic IP

TURN servers need a stable IP address:

```bash
# Allocate Elastic IP
aws ec2 allocate-address --domain vpc

# Note the AllocationId and PublicIp

# Get instance ID
INSTANCE_ID=$(aws elasticbeanstalk describe-environment-resources \
  --environment-name coturn-prod \
  --query 'EnvironmentResources.Instances[0].Id' --output text)

# Associate Elastic IP
aws ec2 associate-address --instance-id $INSTANCE_ID --allocation-id <AllocationId>

# Ensure coturn advertises the public relay address (required behind NAT / Elastic IP)
eb setenv TURN_EXTERNAL_IP=<ELASTIC_IP>
```

If your deployment requires explicit NAT mapping, set `TURN_EXTERNAL_IP=<PUBLIC_IP>/<PRIVATE_IP>` instead.

## 7. Configure Tambourine Server

Add these environment variables to your Tambourine server environment:

```bash
eb setenv \
  TURN_SERVER_URL=turn:<ELASTIC_IP>:3478 \
  TURN_SHARED_SECRET=<same-secret-as-turn-server> \
  TURN_CREDENTIAL_TTL=3600
```

Or update your `.env` file for local development:

```bash
TURN_SERVER_URL=turn:localhost:3478
TURN_SHARED_SECRET=<your-secret>
TURN_CREDENTIAL_TTL=3600
```

## 8. Verify Deployment

### Test TURN Server

Using `turnutils_uclient` (from coturn package):

```bash
# Generate credentials (use same logic as server)
TIMESTAMP=$(($(date +%s) + 3600))
SECRET=<your-secret>
PASSWORD=$(echo -n $TIMESTAMP | openssl dgst -sha1 -hmac $SECRET -binary | base64)

# Test connection
turnutils_uclient -t -u $TIMESTAMP -w $PASSWORD <ELASTIC_IP>
```

### Test ICE Servers Endpoint

The endpoint now requires a registered client UUID:

```bash
SERVER_URL=https://your-tambourine-server

# Register a client UUID
CLIENT_UUID=$(curl -s -X POST "$SERVER_URL/api/client/register" | jq -r '.uuid')

# Request ICE servers with UUID header
curl -H "X-Client-UUID: $CLIENT_UUID" "$SERVER_URL/api/ice-servers"
```

Should return:

```json
{
  "ice_servers": [
    {"urls": "stun:stun.l.google.com:19302"},
    {"urls": "turn:<ip>:3478", "username": "<timestamp>", "credential": "<hmac>"}
  ]
}
```

### End-to-End Test

1. Connect from a restrictive network (corporate NAT, mobile hotspot)
2. Check browser DevTools -> WebRTC internals
3. Look for `relay` candidates in ICE gathering

## Troubleshooting

### Connection Refused

- Verify security groups allow UDP 3478 and 49152-65535
- Check TURN server logs: `eb logs`

### Authentication Failed

- Verify shared secret matches on both servers
- Check timestamp is within TTL window
- Ensure client is generating credentials correctly

### No Relay Candidates

- STUN is working but TURN is not
- Verify the EB environment is single-instance and using host networking
- Verify `TURN_EXTERNAL_IP` is set to your Elastic IP (or `PUBLIC_IP/PRIVATE_IP` mapping)
- Check TURN server is reachable: `nc -u <ip> 3478`
- Verify client is sending credentials

## Cost Considerations

- **Instance**: t3.micro (~$8/month) suitable for low traffic
- **Elastic IP**: Free when attached to running instance
- **Data Transfer**: WebRTC media relayed through TURN incurs outbound data charges

For high traffic, consider:
- Larger instance type
- Multiple TURN servers behind Route 53 GeoDNS
- Managed TURN service (Twilio, Xirsys)
