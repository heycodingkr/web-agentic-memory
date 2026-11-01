SELECT DISTINCT ON (r.id) r.id, r.document, v.document, authors.items, meta.document->'disabled_accounts'
FROM wam_memory_records r
JOIN wam_memory_versions v ON v.memory_id=r.id AND v.version_no=r.active_version
JOIN wam_memory_index i ON i.version_id=v.id AND i.site_host=$1
CROSS JOIN wam_app_meta meta
LEFT JOIN LATERAL ( SELECT jsonb_agg(jsonb_build_object('id',a.id,'account_id',a.user_id) ORDER BY a.id) AS items FROM wam_memory_authors a WHERE a.version_id=v.id AND a.role='author' AND a.user_id IS NOT NULL
) authors ON TRUE
WHERE r.status IN ('active','needs_reverify')
  AND (i.url_pattern='/*' OR i.url_pattern=$2 OR ( right(i.url_pattern,2)='/*' AND ( $2=left(i.url_pattern,length(i.url_pattern)-2) OR left($2,length(i.url_pattern)-1)=left(i.url_pattern,length(i.url_pattern)-2)||'/' ) ))
  AND ($3::text IS NULL OR i.task_category=$3)
  AND ($4::text IS NULL OR i.kind=$4)
ORDER BY r.id
