# Elastic Beanstalk Deployment Configs

All Elastic Beanstalk deployment bundles are centralized here.

## Available bundles

- `elasticbeanstalk/server` - Tambourine/Pipecat server only
- `elasticbeanstalk/turn` - coturn TURN server only
- `elasticbeanstalk/unified` - Tambourine/Pipecat + coturn in one EB environment

## Notes

- Use single-instance EB environments for WebRTC/TURN UDP behavior.
- `unified` is simpler operationally but couples scaling and failure domains.
- `server` + `turn` split environments provide better isolation and independent scaling.
