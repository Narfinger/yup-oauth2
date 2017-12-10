use types::{ApplicationSecret, FlowType, JsonError};

use chrono::Utc;
use hyper;
use hyper::header::ContentType;
use hyper_rustls::HttpsConnector;
use serde_json as json;
use url::form_urlencoded;
use super::Token;
use reqwest;
use std::borrow::BorrowMut;
use std::error::Error;
use std::io::Read;

/// Implements the [Outh2 Refresh Token Flow](https://developers.google.com/youtube/v3/guides/authentication#devices).
///
/// Refresh an expired access token, as obtained by any other authentication flow.
/// This flow is useful when your `Token` is expired and allows to obtain a new
/// and valid access token.
pub struct RefreshFlow<C> {
    client: C,
    result: RefreshResult,
}


/// All possible outcomes of the refresh flow
pub enum RefreshResult {
    /// Indicates connection failure
    Error(reqwest::Error),
    /// The server did not answer with a new token, providing the server message
    RefreshError(String, Option<String>),
    /// The refresh operation finished successfully, providing a new `Token`
    Success(Token),
}

impl<C> RefreshFlow<C>
    where C: BorrowMut<hyper::Client<HttpsConnector>>
{
    pub fn new(client: C) -> RefreshFlow<C> {
        RefreshFlow {
            client: client,
            result: RefreshResult::Error(hyper::Error::TooLarge),
        }
    }

    /// Attempt to refresh the given token, and obtain a new, valid one.
    /// If the `RefreshResult` is `RefreshResult::Error`, you may retry within an interval
    /// of your choice. If it is `RefreshResult:RefreshError`, your refresh token is invalid
    /// or your authorization was revoked. Therefore no further attempt shall be made,
    /// and you will have to re-authorize using the `DeviceFlow`
    ///
    /// # Arguments
    /// * `authentication_url` - URL matching the one used in the flow that obtained
    ///                          your refresh_token in the first place.
    /// * `client_id` & `client_secret` - as obtained when [registering your application](https://developers.google.com/youtube/registering_an_application)
    /// * `refresh_token` - obtained during previous call to `DeviceFlow::poll_token()` or equivalent
    ///
    /// # Examples
    /// Please see the crate landing page for an example.
    pub fn refresh_token(&mut self,
                         flow_type: FlowType,
                         client_secret: &ApplicationSecret,
                         refresh_token: &str)
                         -> &RefreshResult {
        let _ = flow_type;
        if let RefreshResult::Success(_) = self.result {
            return &self.result;
        }
        
        let mut req = String::new();
        form_urlencoded::Serializer::new(&mut req).extend_pairs(&[("client_id", client_secret.client_id.as_ref()),
                                                                  ("client_secret", client_secret.client_secret.as_ref()),
                                                                  ("refresh_token", refresh_token),
                                                                  ("grant_type", "refresh_token")]);

        
        #[derive(Deserialize)]
        struct JsonToken {
            access_token: String,
            token_type: String,
            expires_in: i64,
        }
        
        let client = reqwest::Client::new();
        let response = client.post(&client_secret.token_uri)
            .header(reqwest::header::ContentType("application/x-www-form-urlencoded".parse().unwrap()))
            .body(req)
            .send();

        self.result = match response {
            Err(e) => RefreshResult::RefreshError(e.description().to_owned(), None), //FIXME the none result is not really what we want, we want more of the error
            Ok(res) => {
                let t_result = res.json::<JsonToken>();
                match t_result {
                    Err(e) => RefreshResult::Error(e),
                    Ok(t)  => RefreshResult::Success(Token {
                        access_token: t.access_token,
                        token_type: t.token_type,
                        refresh_token: refresh_token.to_string(),
                        expires_in: None,
                        expires_in_timestamp: Some(Utc::now().timestamp() + t.expires_in),
                    })
                }
            }
        };

        &self.result
    }
}



#[cfg(test)]
mod tests {
    use hyper;
    use std::default::Default;
    use super::*;
    use super::super::FlowType;
    use yup_hyper_mock::{MockStream, SequentialConnector};
        use helper::parse_application_secret;
        use device::GOOGLE_DEVICE_CODE_URL;

    struct MockGoogleRefresh(SequentialConnector);

    impl Default for MockGoogleRefresh {
        fn default() -> MockGoogleRefresh {
            let mut c = MockGoogleRefresh(Default::default());
            c.0.content.push("HTTP/1.1 200 OK\r\n\
                             Server: BOGUS\r\n\
                             \r\n\
                            {\r\n\
                              \"access_token\":\"1/fFAGRNJru1FTz70BzhT3Zg\",\r\n\
                              \"expires_in\":3920,\r\n\
                              \"token_type\":\"Bearer\"\r\n\
                            }"
                .to_string());

            c
        }
    }

    impl hyper::net::NetworkConnector for MockGoogleRefresh {
        type Stream = MockStream;

        fn connect(&self, host: &str, port: u16, scheme: &str) -> ::hyper::Result<MockStream> {
            self.0.connect(host, port, scheme)
        }
    }

    const TEST_APP_SECRET: &'static str = r#"{"installed":{"client_id":"384278056379-tr5pbot1mil66749n639jo54i4840u77.apps.googleusercontent.com","project_id":"sanguine-rhythm-105020","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://accounts.google.com/o/oauth2/token","auth_provider_x509_cert_url":"https://www.googleapis.com/oauth2/v1/certs","client_secret":"QeQUnhzsiO4t--ZGmj9muUAu","redirect_uris":["urn:ietf:wg:oauth:2.0:oob","http://localhost"]}}"#;

    #[test]
    fn refresh_flow() {

        let appsecret = parse_application_secret(&TEST_APP_SECRET.to_string()).unwrap();

        let mut c = hyper::Client::with_connector(<MockGoogleRefresh as Default>::default());
        let mut flow = RefreshFlow::new(&mut c);


        match *flow.refresh_token(FlowType::Device(GOOGLE_DEVICE_CODE_URL.to_string()), &appsecret, "bogus_refresh_token") {
            RefreshResult::Success(ref t) => {
                assert_eq!(t.access_token, "1/fFAGRNJru1FTz70BzhT3Zg");
                assert!(!t.expired());
            }
            _ => unreachable!(),
        }
    }
}
