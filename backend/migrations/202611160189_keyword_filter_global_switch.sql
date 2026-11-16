-- Preserve the meaning of old immutable versions, then default future versions off.
ALTER TABLE keyword_rule_versions ADD COLUMN filter_enabled boolean NOT NULL DEFAULT true;
ALTER TABLE keyword_rule_versions ALTER COLUMN filter_enabled SET DEFAULT false;

WITH published AS (
 INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,reason,filter_enabled)
 SELECT gen_random_uuid(),h.id,h.revision+1,v.entries,
        '引入敏感词拦截总开关，默认关闭',false
 FROM keyword_scope_heads h JOIN keyword_rule_versions v ON v.id=h.version_id
 WHERE h.id='global'
 RETURNING id,scope_id,revision
)
UPDATE keyword_scope_heads h SET version_id=p.id,revision=p.revision
FROM published p WHERE h.id=p.scope_id;

-- Historical dispatches have no recorded decision; never reinterpret their state.
ALTER TABLE orchestration_dispatches ADD COLUMN keyword_decision jsonb;
