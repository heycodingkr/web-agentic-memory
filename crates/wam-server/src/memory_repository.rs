//! Local catalog adapter with optional durable profiles. The catalog remains
//! seeded; a production repository should replace it with PostgreSQL.

mod profile_file;

use crate::ports::{MemoryRecord, ProfileSnapshot, Repository, RepositoryError};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wam_core::{
    keyword::{KeywordVocabulary, TaskVocabulary},
    leaderboard::RankingAccount,
    memory::{MemoryIndex, MemoryKind, MemoryStatus},
    profile::{
        BioBlock, BioDocument, BioInline, CreatorProfile, ExternalUrl, PublicHandle, ServiceLink,
    },
    url_scope::{CanonicalUrl, UrlMode},
};

#[derive(Clone)]
struct Account {
    id: String,
    profile: CreatorProfile,
    public_opt_in: bool,
    points: i64,
}

#[derive(Clone)]
struct State {
    accounts: Vec<Account>,
}

#[derive(Clone)]
pub struct LocalRepository {
    memories: Vec<MemoryRecord>,
    tasks: TaskVocabulary,
    keywords: KeywordVocabulary,
    state: Arc<Mutex<State>>,
    profile_path: Option<Arc<PathBuf>>,
}

/// Compatibility name for the seeded process-local test adapter.
pub type InMemoryRepository = LocalRepository;

impl LocalRepository {
    pub fn seeded() -> Self {
        let tasks = TaskVocabulary::new("2026-01", ["compose-draft", "navigate", "research"])
            .expect("valid task vocabulary");
        let keywords =
            KeywordVocabulary::new("2026-01", ["blog", "draft", "publish", "search", "write"])
                .expect("valid keyword vocabulary");
        let pattern = CanonicalUrl::parse("https://example.com/guide", UrlMode::Public)
            .expect("valid public demo URL")
            .subtree_pattern();
        let category = tasks
            .resolve("compose-draft")
            .expect("seed category exists");
        let memory_keywords = keywords
            .filter(["blog", "write"])
            .expect("seed keywords exist");
        let index = MemoryIndex::new(
            "web:example-draft",
            "v1",
            vec![pattern],
            category,
            MemoryKind::Workflow,
            "create-draft",
            memory_keywords,
            2,
            MemoryStatus::Active,
            1_798_000_000,
        )
        .expect("valid demo memory");
        let memories = vec![MemoryRecord {
            index,
            title: "Save a clean draft".to_owned(),
            summary: "Enter a title and body, then save a draft without publishing it.".to_owned(),
            site: "Example Guide".to_owned(),
            domain: "example.com".to_owned(),
            verified_date: "Local demo seed".to_owned(),
            hint: "Open the editor, add a clear title, then save the draft.".to_owned(),
            detail: "1. Open the editor. 2. Add a clear title and body. 3. Preview the draft. 4. Save without publishing.".to_owned(),
            author_account_id: Some("demo-peer".to_owned()),
        }];
        let accounts = vec![
            Account {
                id: "demo-user".to_owned(),
                profile: demo_profile("agent_jason", "I build useful browser tools."),
                public_opt_in: true,
                points: 50,
            },
            Account {
                id: "demo-peer".to_owned(),
                profile: demo_profile("memory_builder", "I share reliable workflows."),
                public_opt_in: true,
                points: 24,
            },
            Account {
                id: "private-user".to_owned(),
                profile: demo_profile("quiet_builder", "A private profile."),
                public_opt_in: false,
                points: 12,
            },
        ];
        Self {
            memories,
            tasks,
            keywords,
            state: Arc::new(Mutex::new(State { accounts })),
            profile_path: None,
        }
    }

    /// Keeps account profiles and visibility choices in a private local file.
    /// This adapter supports one server process; it does not provide DB transactions.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, RepositoryError> {
        let path = path.into();
        let mut repository = Self::seeded();
        let state = {
            let seed = repository
                .state
                .lock()
                .map_err(|_| RepositoryError::Unavailable)?;
            profile_file::load_or_initialize(&path, &seed)?
        };
        repository.state = Arc::new(Mutex::new(state));
        repository.profile_path = Some(Arc::new(path));
        Ok(repository)
    }

    fn update_accounts(
        &self,
        operation: impl FnOnce(&mut State) -> Result<(), RepositoryError>,
    ) -> Result<(), RepositoryError> {
        let mut current = self
            .state
            .lock()
            .map_err(|_| RepositoryError::Unavailable)?;
        let mut next = current.clone();
        operation(&mut next)?;
        if let Some(path) = &self.profile_path {
            profile_file::persist(path, &next)?;
        }
        *current = next;
        Ok(())
    }
}

fn demo_profile(handle: &str, text: &str) -> CreatorProfile {
    let profile_handle = PublicHandle::parse(handle).expect("valid demo handle");
    let bio = BioDocument::try_new(vec![BioBlock::Paragraph(vec![BioInline::Text(
        text.to_owned(),
    )])])
    .expect("valid demo bio");
    let link = ServiceLink::new(
        "Service",
        ExternalUrl::parse("https://example.com").expect("valid demo link"),
    )
    .expect("valid demo service link");
    CreatorProfile::try_new(profile_handle, bio, vec![link]).expect("valid demo profile")
}

impl Repository for LocalRepository {
    fn memories(&self) -> Vec<MemoryRecord> {
        self.memories.clone()
    }

    fn task_vocabulary(&self) -> TaskVocabulary {
        self.tasks.clone()
    }

    fn keyword_vocabulary(&self) -> KeywordVocabulary {
        self.keywords.clone()
    }

    fn ranking_accounts(&self) -> Vec<RankingAccount> {
        let state = self.state.lock().expect("repository mutex poisoned");
        state
            .accounts
            .iter()
            .map(|account| {
                RankingAccount::new(
                    account.id.clone(),
                    account.points,
                    Some(account.profile.handle().clone()),
                    account.public_opt_in,
                )
            })
            .collect()
    }

    fn profile_by_handle(&self, handle: &PublicHandle) -> Option<ProfileSnapshot> {
        let state = self.state.lock().ok()?;
        state
            .accounts
            .iter()
            .find(|account| account.profile.handle() == handle)
            .map(|account| ProfileSnapshot {
                profile: account.profile.clone(),
                public_opt_in: account.public_opt_in,
            })
    }

    fn profile_for_account(&self, account_id: &str) -> Option<ProfileSnapshot> {
        let state = self.state.lock().ok()?;
        state
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .map(|account| ProfileSnapshot {
                profile: account.profile.clone(),
                public_opt_in: account.public_opt_in,
            })
    }

    fn ensure_account(&self, account_id: &str) -> Result<(), RepositoryError> {
        // Existing accounts need no disk write on subsequent authentication.
        {
            let state = self
                .state
                .lock()
                .map_err(|_| RepositoryError::Unavailable)?;
            if state
                .accounts
                .iter()
                .any(|account| account.id == account_id)
            {
                return Ok(());
            }
        }
        self.update_accounts(|state| {
            if state
                .accounts
                .iter()
                .any(|account| account.id == account_id)
            {
                return Ok(());
            }
            let suffix = account_id
                .strip_prefix("github_")
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= 128
                        && value
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                })
                .ok_or(RepositoryError::AccountMissing)?;
            let base = format!("member_{}", suffix.chars().take(10).collect::<String>());
            // An existing or user-selected handle must not prevent another login.
            let mut handle = base.clone();
            for number in 1..=state.accounts.len() + 1 {
                if !state
                    .accounts
                    .iter()
                    .any(|account| account.profile.handle().as_str() == handle)
                {
                    break;
                }
                handle = format!("{base}_{number}");
            }
            PublicHandle::parse(&handle).map_err(|_| RepositoryError::HandleTaken)?;
            state.accounts.push(Account {
                id: account_id.to_owned(),
                profile: demo_profile(&handle, "A WAM member."),
                public_opt_in: false,
                points: 0,
            });
            Ok(())
        })
    }

    fn save_profile(
        &self,
        account_id: &str,
        profile: CreatorProfile,
        public_opt_in: bool,
    ) -> Result<(), RepositoryError> {
        self.update_accounts(|state| {
            if state.accounts.iter().any(|account| {
                account.id != account_id && account.profile.handle() == profile.handle()
            }) {
                return Err(RepositoryError::HandleTaken);
            }
            let account = state
                .accounts
                .iter_mut()
                .find(|account| account.id == account_id)
                .ok_or(RepositoryError::AccountMissing)?;
            account.profile = profile;
            account.public_opt_in = public_opt_in;
            Ok(())
        })
    }
}
