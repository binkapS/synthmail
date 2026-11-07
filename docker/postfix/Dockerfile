FROM postfix:3.9
RUN apt-get update && apt-get install -y --no-install-recommends \
    postfix-pcre supervisor \
    && rm -rf /var/lib/apt/lists/*
COPY main.cf /etc/postfix/main.cf
COPY master.cf /etc/postfix/master.cf
# entrypoint renders cert paths from certs volume, then starts postfix
ENTRYPOINT ["/bin/sh", "-c", "postfix start; tail -f /var/log/mail.log || sleep infinity"]
EXPOSE 25 465 587
