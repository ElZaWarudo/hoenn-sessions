-- Small-pilot repository format, independent of the future normalized schema.
-- Only one application process may own the advisory lock for this database.
CREATE TABLE IF NOT EXISTS coop_pilot_checkpoint (
    id integer PRIMARY KEY CHECK (id = 1),
    format_version integer NOT NULL CHECK (format_version > 0),
    payload bytea NOT NULL CHECK (octet_length(payload) <= 33554432),
    updated_at timestamptz NOT NULL DEFAULT now()
);
