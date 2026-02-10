# AWS Elastic Beanstalk Deployment

This folder contains the configuration files for deploying the Tambourine server to AWS Elastic Beanstalk using a Docker image from ECR.

## Prerequisites

1. AWS CLI configured (`aws configure`)
2. Docker image pushed to AWS ECR (follow the push instructions in the ECR Console)

## Setup Steps

### 1. Configure Dockerrun.aws.json

Edit `Dockerrun.aws.json` and replace `<YOUR_ECR_URI>` with your ECR repository URI (found in the ECR Console):

### 2. Create Deployment ZIP

```bash
cd elasticbeanstalk/server
zip -r tambourine-eb-config.zip Dockerrun.aws.json .ebextensions/
```

### 3. Create Elastic Beanstalk Environment (AWS Console)

1. Go to [AWS Elastic Beanstalk Console](https://console.aws.amazon.com/elasticbeanstalk)
2. Click **Create application**
3. Configure:
   - **Application name:** `tambourine-server`
   - **Platform:** Docker
   - **Platform branch:** Docker running on 64bit Amazon Linux 2023
   - **Application code:** Upload `tambourine-eb-config.zip`

4. **CRITICAL:** Click **Configure more options** before creating:
   - Find **Capacity** → Click **Edit**
   - Change **Environment type** to **Single instance** (required for WebRTC)
   - Click **Save**

5. Click **Create application** and wait for it to become healthy

### 4. Grant ECR Permissions

1. Go to **IAM → Roles**
2. Find `aws-elasticbeanstalk-ec2-role`
3. Click **Add permissions → Attach policies**
4. Attach: `AmazonEC2ContainerRegistryReadOnly`
5. Restart the EB environment

### 5. Add Environment Variables

1. Go to **EB → Your environment → Configuration**
2. Find **Software** → Click **Edit**
3. Scroll to **Environment properties**
4. Add your API keys
5. Click **Apply**

## Updating the Server

After pushing a new image to ECR:

1. Go to EB Console → Your environment
2. Click **Actions → Restart app server(s)**

EB will automatically pull the latest image (due to `"Update": "true"` in Dockerrun.aws.json).

## Files

| File                               | Purpose                          |
| ---------------------------------- | -------------------------------- |
| `Dockerrun.aws.json`               | Tells EB which ECR image to pull |
| `.ebextensions/01-firewall.config` | Opens UDP ports for WebRTC media |

## Important Notes

- **Single instance is required** - Load balancers break WebRTC UDP connections
- The server uses Google STUN (`stun:stun.l.google.com:19302`) for NAT traversal
- Security groups open UDP 32768-65535 and TCP 8765 to all IPs (0.0.0.0/0)
