-- Additive archive for one-time operator maintenance of the pilot checkpoint.
-- Applied only by `coop-server fresh-start`, inside the same transaction that
-- archives the original payload and rewrites coop_pilot_checkpoint.
CREATE TABLE IF NOT EXISTS coop_pilot_checkpoint_archive (
    id bigserial PRIMARY KEY,
    archived_at timestamptz NOT NULL DEFAULT now(),
    reason text NOT NULL,
    format_version integer NOT NULL,
    payload bytea NOT NULL CHECK (octet_length(payload) <= 33554432),
    payload_sha256 bytea NOT NULL UNIQUE CHECK (octet_length(payload_sha256) = 32)
);
