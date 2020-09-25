//! This module implements a bunch of utilities that help Pencil
//! to deal with HTTP data.

use hyper::StatusCode;


/// Get HTTP status name by status code.
pub fn get_name_by_http_code(code: u16) -> Option<&'static str> {
    get_status_from_code(code).canonical_reason()
}


/// Return the full content type with charset for a mimetype.
pub fn get_content_type(mimetype: &str, charset: &str) -> String {
    if (mimetype.starts_with("text/") || (mimetype == "application/xml") ||
       (mimetype.starts_with("application/") && mimetype.ends_with("+xml"))) &&
       !mimetype.contains("charset") {
        mimetype.to_string() + "; charset=" + charset
    } else {
        mimetype.to_string()
    }
}

/// Return the status code used by hyper response.
pub fn get_status_from_code(code: u16) -> StatusCode {
    StatusCode::from_u16(code).expect("TODO")
}


#[test]
fn test_get_name_by_http_code() {
    let status_name = get_name_by_http_code(200).unwrap();
    assert!(status_name == "OK");
}
