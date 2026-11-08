-- Final retirement of forum/school storage and unused legacy workflows.
-- Deliberately destructive: apply after the linjian migration and database backup.
-- Existing migrations remain unchanged; SQLx applies this transaction once.
-- Keep independent workspace IDs, confirmed content, execution grants and results.
DROP TRIGGER forum_distribution_material_changed ON posts;
DROP FUNCTION forum_distribution_material_changed();

-- Retain historical claim IDs as evidence in live invocation/diagnostic records,
-- while retiring the old claim/prepare protocol and its unused tables.
ALTER TABLE mobile_v9_invocations DROP CONSTRAINT mobile_v9_invocations_school_id_claim_id_fkey;
ALTER TABLE mobile_v9_diagnostics DROP CONSTRAINT mobile_v9_diagnostics_claim_id_fkey;
ALTER TABLE accounts DROP COLUMN bound_school_id;

-- Explicit dependency closure; RESTRICT is intentional. Unexpected dependencies
-- fail the migration transaction rather than deleting other objects with CASCADE.
DROP TABLE
 comment_reactions,
 comments,
 delivery_attempts,
 delivery_draft_posts,
 delivery_drafts,
 historical_delivery_events,
 migration_runs,
 mobile_shared_device_leases,
 mobile_v9_dispatch_failures,
 mobile_v9_preparations,
 mobile_v9_scheduler_turns,
 mobile_v9_send_claims,
 mobile_v9_settlements,
 moderation_events,
 notifications,
 operational_media,
 post_reactions,
 resource_publications,
 retired_mobile_incident_archive,
 school_memberships,
 school_resources,
 schools,
 posts,
 cases,
 follows,
 legacy_id_map,
 legacy_aliases;
-- legacy_public_id is owned by legacy_aliases.public_id and drops with the table.
