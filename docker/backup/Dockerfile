# SynthMail backup image: pg_dump + tar + sha256 + retention + optional rclone.
# Audit H-09: the previous image (rclone/rclone) had no pg_dump and swallowed errors.
FROM alpine:3.20
RUN apk add --no-cache postgresql15-client rclone tar
COPY run-backup.sh /usr/local/bin/run-backup.sh
COPY restore.sh /usr/local/bin/restore.sh
RUN chmod +x /usr/local/bin/run-backup.sh /usr/local/bin/restore.sh
ENTRYPOINT ["/usr/local/bin/run-backup.sh"]
