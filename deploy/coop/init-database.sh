#!/usr/bin/env bash
set -euo pipefail
export COOP_APP_PASSWORD
COOP_APP_PASSWORD=$(cat /run/secrets/database_password)
# Runs only when the PostgreSQL volume is first initialized. Keep the admin
# credential out of the application container and URL.
psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" --set ON_ERROR_STOP=1 <<'SQL'
\getenv app_password COOP_APP_PASSWORD
CREATE ROLE coop LOGIN PASSWORD :'app_password' NOSUPERUSER NOCREATEDB NOCREATEROLE;
ALTER DATABASE coop OWNER TO coop;
SQL
unset COOP_APP_PASSWORD
