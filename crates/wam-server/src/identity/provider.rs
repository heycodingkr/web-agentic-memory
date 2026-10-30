use super::IdentityError;
use async_trait::async_trait;
use reqwest::{Client, header};
use serde::Deserialize;
use url::Url;

#[async_trait]
pub trait GitHubOAuthProvider: Send + Sync + 'static {
    fn authorization_url(&self, state: &str, code_challenge: &str)
    -> Result<String, IdentityError>;
    async fn exchange_code(&self, code: &str, code_verifier: &str)
    -> Result<String, IdentityError>;
}

pub(super) struct GitHubOAuthClient {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    token_endpoint: Url,
    user_endpoint: Url,
    client: Client,
}

impl GitHubOAuthClient {
    pub(super) fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
    ) -> Result<Self, IdentityError> {
        let token_endpoint = Url::parse("https://github.com/login/oauth/access_token")
            .map_err(|_| IdentityError::Provider)?;
        let user_endpoint =
            Url::parse("https://api.github.com/user").map_err(|_| IdentityError::Provider)?;
        Self::with_endpoints(
            client_id,
            client_secret,
            redirect_uri,
            token_endpoint,
            user_endpoint,
        )
    }

    fn with_endpoints(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        token_endpoint: Url,
        user_endpoint: Url,
    ) -> Result<Self, IdentityError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("wam-server/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .map_err(|_| IdentityError::Provider)?;
        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            token_endpoint,
            user_endpoint,
            client,
        })
    }
}

#[derive(Deserialize)]
struct GitHubTokenResponse {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct GitHubUserResponse {
    id: u64,
}

#[async_trait]
impl GitHubOAuthProvider for GitHubOAuthClient {
    fn authorization_url(
        &self,
        state: &str,
        code_challenge: &str,
    ) -> Result<String, IdentityError> {
        let mut url = Url::parse("https://github.com/login/oauth/authorize")
            .map_err(|_| IdentityError::Provider)?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", "read:user")
            .append_pair("state", state)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<String, IdentityError> {
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .header(header::ACCEPT, "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await
            .map_err(|_| IdentityError::Provider)?;
        if !response.status().is_success() {
            return Err(IdentityError::Provider);
        }
        let token: GitHubTokenResponse =
            response.json().await.map_err(|_| IdentityError::Provider)?;
        let access_token = token.access_token.ok_or(IdentityError::Provider)?;
        let response = self
            .client
            .get(self.user_endpoint.clone())
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| IdentityError::Provider)?;
        if !response.status().is_success() {
            return Err(IdentityError::Provider);
        }
        let user: GitHubUserResponse =
            response.json().await.map_err(|_| IdentityError::Provider)?;
        Ok(user.id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread::{self, JoinHandle},
        time::Duration,
    };

    fn read_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    fn serve(
        replies: Vec<(&'static str, &'static str)>,
    ) -> (String, mpsc::Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                sender.send(read_request(&mut stream)).unwrap();
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (endpoint, receiver, server)
    }

    fn fixture_provider(base: &str) -> GitHubOAuthClient {
        GitHubOAuthClient::with_endpoints(
            "fixture-client".into(),
            "fixture-client-secret".into(),
            "https://wam.example/auth/callback".into(),
            Url::parse(&format!("{base}/login/oauth/access_token")).unwrap(),
            Url::parse(&format!("{base}/user")).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn authorization_url_encodes_pkce_state_and_redirect_parameters() {
        let provider = GitHubOAuthClient::new(
            "client id".into(),
            "client-secret-must-stay-private".into(),
            "https://wam.example/auth/callback?next=home".into(),
        )
        .unwrap();
        let url = Url::parse(
            &provider
                .authorization_url("state &?/", "challenge +?")
                .unwrap(),
        )
        .unwrap();
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();

        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("github.com"));
        assert_eq!(url.path(), "/login/oauth/authorize");
        assert_eq!(
            query.get("client_id").map(String::as_str),
            Some("client id")
        );
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some("https://wam.example/auth/callback?next=home")
        );
        assert_eq!(query.get("scope").map(String::as_str), Some("read:user"));
        assert_eq!(query.get("state").map(String::as_str), Some("state &?/"));
        assert_eq!(
            query.get("code_challenge").map(String::as_str),
            Some("challenge +?")
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(!url.as_str().contains("client-secret-must-stay-private"));
        assert!(!query.contains_key("code_verifier"));
    }

    #[tokio::test]
    async fn code_exchange_posts_pkce_data_then_uses_the_token_for_the_user_lookup() {
        let (base, requests, server) = serve(vec![
            ("200 OK", r#"{"access_token":"fixture-user-token"}"#),
            ("200 OK", r#"{"id":123456}"#),
        ]);
        let provider = fixture_provider(&base);

        assert_eq!(
            provider
                .exchange_code("code /?&", "verifier+?")
                .await
                .unwrap(),
            "123456"
        );

        let token_request = requests.recv().unwrap();
        assert!(token_request.starts_with("POST /login/oauth/access_token HTTP/1.1\r\n"));
        let token_form: HashMap<_, _> =
            url::form_urlencoded::parse(token_request.split_once("\r\n\r\n").unwrap().1.as_bytes())
                .into_owned()
                .collect();
        assert_eq!(
            token_form.get("client_id").map(String::as_str),
            Some("fixture-client")
        );
        assert_eq!(
            token_form.get("client_secret").map(String::as_str),
            Some("fixture-client-secret")
        );
        assert_eq!(token_form.get("code").map(String::as_str), Some("code /?&"));
        assert_eq!(
            token_form.get("code_verifier").map(String::as_str),
            Some("verifier+?")
        );
        assert_eq!(
            token_form.get("redirect_uri").map(String::as_str),
            Some("https://wam.example/auth/callback")
        );
        assert!(
            !token_request
                .lines()
                .next()
                .unwrap()
                .contains("fixture-client-secret")
        );

        let user_request = requests.recv().unwrap();
        assert!(user_request.starts_with("GET /user HTTP/1.1\r\n"));
        assert!(
            user_request
                .to_ascii_lowercase()
                .contains("authorization: bearer fixture-user-token")
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn provider_http_errors_are_closed_and_do_not_echo_response_secrets() {
        let (base, requests, server) = serve(vec![(
            "400 Bad Request",
            r#"{"error":"bad_verifier","message":"provider-private-detail"}"#,
        )]);
        let provider = fixture_provider(&base);
        let error = provider
            .exchange_code("fixture-code", "fixture-verifier")
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), "identity provider request failed");
        let token_request = requests.recv().unwrap();
        assert!(token_request.starts_with("POST /login/oauth/access_token HTTP/1.1\r\n"));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn token_exchange_does_not_forward_credentials_across_a_redirect() {
        let token_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let target_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", token_listener.local_addr().unwrap());
        let target = format!("http://{}", target_listener.local_addr().unwrap());
        target_listener.set_nonblocking(true).unwrap();
        let (sender, receiver) = mpsc::channel();

        let redirect_target = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_millis(250);
            loop {
                match target_listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_request(&mut stream);
                        sender.send(request).unwrap();
                        stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 25\r\nConnection: close\r\n\r\n{\"access_token\":\"leaked\"}",
                        ).unwrap();
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("redirect listener failed: {error}"),
                }
            }
        });

        let redirect = thread::spawn(move || {
            let (mut stream, _) = token_listener.accept().unwrap();
            let request = read_request(&mut stream);
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 307 Temporary Redirect\r\nLocation: {target}/capture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .unwrap();
            request
        });

        let provider = fixture_provider(&base);
        assert!(matches!(
            provider
                .exchange_code("fixture-code", "fixture-verifier")
                .await,
            Err(IdentityError::Provider)
        ));
        let original_request = redirect.join().unwrap();
        assert!(original_request.starts_with("POST /login/oauth/access_token HTTP/1.1\r\n"));
        redirect_target.join().unwrap();
        assert!(receiver.try_recv().is_err());
    }
}
