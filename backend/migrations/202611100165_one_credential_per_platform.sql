-- A caller identity represents one platform and has one credential.
CREATE UNIQUE INDEX source_credentials_one_per_platform ON source_credentials(source_id);
