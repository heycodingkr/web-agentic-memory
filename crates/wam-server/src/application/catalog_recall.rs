use super::*;
use wam_core::KeywordVocabulary;

impl<R: Repository> WamService<R> {
    pub fn vocab(&self) -> VocabResponse {
        self.make_vocab(self.repository.keyword_vocabulary())
    }

    pub fn try_vocab(&self) -> Result<VocabResponse, ServiceError> {
        let keyword_vocabulary = self
            .repository
            .try_keyword_vocabulary()
            .map_err(|_| ServiceError::Unavailable)?;
        Ok(self.make_vocab(keyword_vocabulary))
    }

    fn make_vocab(&self, keyword_vocabulary: KeywordVocabulary) -> VocabResponse {
        let task_vocabulary = self.repository.task_vocabulary();
        VocabResponse {
            schema_version: SCHEMA_VERSION,
            version: keyword_vocabulary.version().to_owned(),
            task_categories: task_vocabulary.codes().map(str::to_owned).collect(),
            keywords: keyword_vocabulary.values().map(str::to_owned).collect(),
        }
    }

    pub fn catalog(&self) -> Result<MemoryCatalogResponse, ServiceError> {
        let mut public_authors = std::collections::HashMap::new();
        let memories = self
            .repository
            .try_memories()
            .map_err(|_| ServiceError::Unavailable)?
            .into_iter()
            .filter(|record| {
                matches!(
                    record.index.status(),
                    MemoryStatus::Active | MemoryStatus::NeedsReverify
                )
            })
            .map(|record| {
                let author_handle = if let Some(account) = record.author_account_id.as_deref() {
                    if !public_authors.contains_key(account) {
                        let handle = self
                            .repository
                            .try_profile_for_account(account)
                            .map_err(|_| ServiceError::Unavailable)?
                            .filter(|snapshot| snapshot.public_opt_in)
                            .map(|snapshot| snapshot.profile.handle().as_str().to_owned());
                        public_authors.insert(account.to_owned(), handle);
                    }
                    public_authors[account].clone()
                } else {
                    None
                };
                Ok(MemoryCatalogEntry {
                    memory_id: record.index.id().to_owned(),
                    version_id: record.index.version_id().to_owned(),
                    title: record.title,
                    summary: record.summary,
                    site: record.site,
                    domain: record.domain,
                    task_category: record.index.task_category().as_str().to_owned(),
                    kind: memory_kind_name(record.index.kind()).to_owned(),
                    status: memory_status_name(record.index.status()).to_owned(),
                    verified_date: record.verified_date,
                    canonical_url_patterns: record
                        .index
                        .url_patterns()
                        .iter()
                        .map(format_url_pattern)
                        .collect(),
                    author_handle,
                })
            })
            .collect::<Result<_, ServiceError>>()?;
        Ok(MemoryCatalogResponse {
            schema_version: SCHEMA_VERSION,
            memories,
        })
    }

    pub fn recall(
        &self,
        request: RecallRequest,
        account_id: Option<&str>,
    ) -> Result<RecallResponse, ServiceError> {
        if request.url.len() > 2048 {
            return Err(ServiceError::InvalidInput("invalid_url"));
        }
        let url = CanonicalUrl::parse(&request.url, UrlMode::Public)
            .map_err(|_| ServiceError::InvalidInput("invalid_url"))?;
        let tasks = self.repository.task_vocabulary();
        let task = request
            .task_category
            .as_deref()
            .map(|value| {
                tasks
                    .resolve(value)
                    .map_err(|_| ServiceError::InvalidInput("invalid_task_category"))
            })
            .transpose()?;
        let keywords = self
            .repository
            .try_keyword_vocabulary()
            .map_err(|_| ServiceError::Unavailable)?
            .filter(request.keywords)
            .map_err(|_| ServiceError::InvalidInput("invalid_keywords"))?;
        let mut query = RecallQuery::new(url, task, parse_kind(request.kind.as_deref())?, keywords)
            .map_err(|_| ServiceError::InvalidInput("invalid_recall_query"))?;
        if let Some(limit) = request.limit {
            query = query
                .with_limit(limit)
                .map_err(|_| ServiceError::InvalidInput("invalid_limit"))?;
        }

        let records = self
            .repository
            .try_recall_candidates(&query)
            .map_err(|_| ServiceError::Unavailable)?;
        let documents: Vec<_> = records
            .iter()
            .map(|record| {
                RecallDocument::new(
                    &record.index,
                    &record.title,
                    &record.summary,
                    &record.site,
                    &record.domain,
                )
            })
            .collect();
        let hits: Vec<RecallHit> = recall_documents(&documents, &query)
            .into_iter()
            .filter_map(|hit| {
                let record = records
                    .iter()
                    .find(|record| record.index.id() == hit.memory.id())?;
                Some(RecallHit {
                    memory_id: hit.memory.id().to_owned(),
                    version_id: hit.memory.version_id().to_owned(),
                    task_category: hit.memory.task_category().as_str().to_owned(),
                    kind: memory_kind_name(hit.memory.kind()).to_owned(),
                    hint: record.hint.clone(),
                    score: hit.score,
                    matched_on: MatchDetails {
                        route: route_name(hit.matched_on.route).to_owned(),
                        task: hit.matched_on.task,
                        keyword_matches: hit.matched_on.keyword_matches,
                        text_matches: hit.matched_on.text_matches,
                    },
                })
            })
            .collect();
        let memory_uses: Vec<_> = hits
            .iter()
            .filter_map(|hit| {
                let record = records
                    .iter()
                    .find(|record| record.index.version_id() == hit.version_id)?;
                Some(ReceiptMemoryAttribution {
                    memory_id: hit.memory_id.clone(),
                    version_id: hit.version_id.clone(),
                    author_account_id: record.author_account_id.clone(),
                })
            })
            .collect();
        let receipt_id = self
            .usage
            .record_recall(account_id, &memory_uses, now_unix())
            .map_err(|_| ServiceError::Internal)?;
        Ok(RecallResponse {
            schema_version: SCHEMA_VERSION,
            reason: "ok".to_owned(),
            hits,
            receipt_id,
            cache_status: None,
        })
    }

    pub fn expand(
        &self,
        id: &str,
        level: u8,
        account_id: Option<&str>,
    ) -> Result<MemoryDetailResponse, ServiceError> {
        self.expand_version(id, level, None, account_id)
    }

    pub fn expand_version(
        &self,
        id: &str,
        level: u8,
        version_id: Option<&str>,
        account_id: Option<&str>,
    ) -> Result<MemoryDetailResponse, ServiceError> {
        if !matches!(level, 1 | 2) {
            return Err(ServiceError::InvalidInput("invalid_level"));
        }
        if version_id.is_some_and(|version| version.is_empty() || version.len() > 64) {
            return Err(ServiceError::InvalidInput("invalid_version_id"));
        }
        let record = match version_id {
            Some(version_id) => self.repository.try_memory_by_version(id, version_id),
            None => self.repository.try_memory_by_id(id),
        }
        .map_err(|_| ServiceError::Unavailable)?
        .filter(|record| {
            matches!(
                record.index.status(),
                MemoryStatus::Active | MemoryStatus::NeedsReverify
            )
        })
        .ok_or(ServiceError::NotFound)?;
        self.usage
            .record_detail(
                &ReceiptMemoryAttribution {
                    memory_id: record.index.id().to_owned(),
                    version_id: record.index.version_id().to_owned(),
                    author_account_id: record.author_account_id.clone(),
                },
                account_id,
                now_unix(),
            )
            .map_err(|_| ServiceError::Internal)?;
        let content = if level == 1 {
            record.hint
        } else {
            record.detail
        };
        Ok(MemoryDetailResponse {
            schema_version: SCHEMA_VERSION,
            memory_id: record.index.id().to_owned(),
            version_id: record.index.version_id().to_owned(),
            level,
            content,
        })
    }
}
