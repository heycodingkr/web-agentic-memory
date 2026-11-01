//! Run only against an owned disposable database; each invocation owns its schema.
use std::sync::{Arc, Mutex};
use wam_server::identity::{IdentityError, email::EmailSender};
use wam_server::{ApplicationRepository, FileIdentityStore, IdentityService};

#[derive(Default)]
struct Mailer(Mutex<String>, Mutex<String>);
#[async_trait::async_trait]
impl EmailSender for Mailer {
    async fn send_admin_link(&self, _: &str, link: &str, _: &str) -> Result<(), IdentityError> {
        *self.1.lock().unwrap() = link.split("#token=").nth(1).unwrap().into();
        Ok(())
    }
    async fn send_code(&self, _: &str, code: &str, _: &str) -> Result<(), IdentityError> {
        *self.0.lock().unwrap() = code.into();
        Ok(())
    }
}
struct Schema {
    base: String,
    name: String,
    url: String,
}
impl Schema {
    fn new() -> Self {
        let base = std::env::var("WAM_TEST_DATABASE_URL")
            .expect("use an owned disposable WAM_TEST_DATABASE_URL");
        let mut random = [0; 10];
        getrandom::fill(&mut random).unwrap();
        let name = format!(
            "wam_email_{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        postgres::Client::connect(&base, postgres::NoTls)
            .unwrap()
            .batch_execute(&format!("CREATE SCHEMA {name}"))
            .unwrap();
        let mut url = url::Url::parse(&base).unwrap();
        url.query_pairs_mut()
            .append_pair("options", &format!("-c search_path={name}"));
        let query = url.query().unwrap().replace('+', "%20");
        url.set_query(Some(&query));
        Self {
            base,
            name,
            url: url.to_string(),
        }
    }
    fn identity(&self, mailer: Arc<Mailer>) -> IdentityService {
        IdentityService::new(
            Arc::new(FileIdentityStore::from_repository(
                ApplicationRepository::connect(&self.url).unwrap(),
            )),
            None,
            "https://wam.example",
            true,
        )
        .with_email_auth(mailer, "fixture-secret-at-least-32-bytes-long".into())
        .unwrap()
    }
}
impl Drop for Schema {
    fn drop(&mut self) {
        let _ = postgres::Client::connect(&self.base, postgres::NoTls).and_then(|mut client| {
            client.batch_execute(&format!("DROP SCHEMA {} CASCADE", self.name))
        });
    }
}

#[test]
#[ignore = "requires owned disposable WAM_TEST_DATABASE_URL"]
fn admin_links_and_sessions_are_shared_one_time_and_revocable_across_postgres_replicas() {
    let schema = Schema::new();
    let mailer = Arc::new(Mailer::default());
    let identity = || {
        schema
            .identity(mailer.clone())
            .with_admin_auth("admin@example.test", "https://admin.wam.example")
            .unwrap()
    };
    let first = identity();
    let second = identity();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let binding = runtime
        .block_on(first.begin_admin("admin@example.test", 1000))
        .unwrap();
    let token = mailer.1.lock().unwrap().clone();
    assert_eq!(
        runtime.block_on(second.begin_admin("admin@example.test", 1001)),
        Err(IdentityError::RateLimited)
    );
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| first.finish_admin(&binding, &token, 1001));
        let b = scope.spawn(|| second.finish_admin(&binding, &token, 1001));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let (token, _) = a.or(b).unwrap();
    let reopened = identity();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "cookie",
        format!("__Host-wam_admin_session={token}").parse().unwrap(),
    );
    assert!(reopened.authenticate_admin(&headers, 1002).is_ok());
    reopened.logout_admin(&headers).unwrap();
    assert!(first.authenticate_admin(&headers, 1003).is_err());
    assert!(second.authenticate_admin(&headers, 1003).is_err());
}

#[test]
#[ignore = "requires owned disposable WAM_TEST_DATABASE_URL"]
fn email_identity_limits_and_consumption_are_shared_across_postgres_replicas() {
    let schema = Schema::new();
    let mailer = Arc::new(Mailer::default());
    let first = schema.identity(mailer.clone());
    let second = schema.identity(mailer.clone());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let challenge = runtime
        .block_on(first.begin_email("person@example.com", 1000))
        .unwrap();
    let code = mailer.0.lock().unwrap().clone();
    assert_eq!(
        runtime.block_on(second.begin_email("PERSON@EXAMPLE.COM", 1001)),
        Err(IdentityError::RateLimited)
    );
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| first.finish_email(&challenge, &code, 1001));
        let b = scope.spawn(|| second.finish_email(&challenge, &code, 1001));
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let (account, token, csrf, _) = a.or(b).unwrap();
    assert!(account.starts_with("email_"));
    let reopened = schema.identity(mailer.clone());
    assert_eq!(reopened.session_info(&token, 1002).unwrap().0, account);
    assert!(reopened.csrf_valid(&token, &csrf, 1002));
    let challenge = runtime
        .block_on(reopened.begin_email("person@example.com", 1060))
        .unwrap();
    let code = mailer.0.lock().unwrap().clone();
    assert_eq!(
        reopened.finish_email(&challenge, &code, 1061).unwrap().0,
        account
    );
}
