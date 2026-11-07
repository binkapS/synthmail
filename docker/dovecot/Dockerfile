FROM dovecot/dovecot:2.3
RUN apt-get update && apt-get install -y --no-install-recommends \
    dovecot-lda dovecot-sieve dovecot-sql \
    && rm -rf /var/lib/apt/lists/*
COPY dovecot.conf /etc/dovecot/dovecot.conf
COPY sql.conf /etc/dovecot/dovecot-sql.conf.ext
EXPOSE 143 993 110 995 4190
