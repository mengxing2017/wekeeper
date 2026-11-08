-- Post-migration checks against populated historical and current business data.
DO $$ DECLARE retired_name text; BEGIN
 FOREACH retired_name IN ARRAY ARRAY['comment_reactions','comments','delivery_attempts','delivery_draft_posts','delivery_drafts','historical_delivery_events','migration_runs','mobile_shared_device_leases','mobile_v9_dispatch_failures','mobile_v9_preparations','mobile_v9_scheduler_turns','mobile_v9_send_claims','mobile_v9_settlements','moderation_events','notifications','operational_media','post_reactions','resource_publications','retired_mobile_incident_archive','school_memberships','school_resources','schools','posts','cases','follows','legacy_id_map','legacy_aliases'] LOOP
  IF to_regclass(retired_name) IS NOT NULL THEN RAISE EXCEPTION 'retired table remains: %',retired_name; END IF;
 END LOOP;
 IF to_regclass('legacy_public_id') IS NOT NULL OR to_regprocedure('forum_distribution_material_changed()') IS NOT NULL THEN RAISE EXCEPTION 'retired sequence or producer remains'; END IF;
 IF EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='accounts' AND column_name='bound_school_id') THEN RAISE EXCEPTION 'account still depends on school'; END IF;
 IF NOT EXISTS(SELECT 1 FROM accounts WHERE id='20000000-0000-4000-8000-000000000002' AND active) THEN RAISE EXCEPTION 'distribution login lost'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_workspaces WHERE id='20000000-0000-4000-8000-000000000001' AND active) OR NOT EXISTS(SELECT 1 FROM wecom_workspaces WHERE id='20000000-0000-4000-8000-000000000001' AND active) THEN RAISE EXCEPTION 'live workspace lost'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_workspace_members WHERE workspace_id='20000000-0000-4000-8000-000000000001') OR NOT EXISTS(SELECT 1 FROM wecom_workspace_members WHERE school_id='20000000-0000-4000-8000-000000000001') THEN RAISE EXCEPTION 'distribution membership lost'; END IF;
 IF NOT EXISTS(SELECT 1 FROM mobile_targets WHERE id='20000000-0000-4000-8000-000000000003' AND search_text='Legacy target') THEN RAISE EXCEPTION 'phone target lost'; END IF;
 IF NOT EXISTS(SELECT 1 FROM execution_devices WHERE id='20000000-0000-4000-8000-000000000014' AND NOT enabled) THEN RAISE EXCEPTION 'phone state changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM broadcast_tasks t JOIN broadcast_batches b ON b.task_id=t.id JOIN broadcast_targets x ON x.batch_id=b.id WHERE t.id='20000000-0000-4000-8000-000000000005' AND t.content='Legacy frozen content' AND t.content_hash='legacy-hash' AND b.msgid='legacy-msgid' AND x.official_status=1) THEN RAISE EXCEPTION 'official sending evidence changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM distribution_message_snapshots WHERE id='20000000-0000-4000-8000-000000000012' AND content='Preserved immutable message') THEN RAISE EXCEPTION 'message snapshot changed'; END IF;
 IF NOT EXISTS(SELECT 1 FROM distribution_materials WHERE external_content_id='20000000-0000-4000-8000-000000000010' AND content='Frozen material body' AND allowed) THEN RAISE EXCEPTION 'distribution projection lost'; END IF;
 IF NOT EXISTS(SELECT 1 FROM audit_events WHERE id='20000000-0000-4000-8000-000000000013' AND detail->>'preserve'='true') THEN RAISE EXCEPTION 'system audit history lost'; END IF;
END $$;
