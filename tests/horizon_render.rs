// SPDX-License-Identifier: GPL-3.0-or-later
use horizon_client_render::{
    auth::{self, AuthSession, JoinError},
    vfp_render_compat,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};

#[test]
fn authentication_retries_transient_failures_without_retrying_invalid_sessions() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let discovery = format!(
        r#"{{"discovery":{{"session":{{"endpoints":{{"join":{{"uri":"{origin}/join"}}}}}}}}}}"#
    );
    let server = std::thread::spawn(move || {
        let mut bodies = Vec::new();
        for (status, response) in [
            (200, discovery.as_str()),
            (503, "{}"),
            (500, "{}"),
            (204, ""),
            (403, r#"{"error":"ForbiddenOperationException"}"#),
            (503, "{}"),
            (503, "{}"),
            (503, "{}"),
            (503, "{}"),
            (204, ""),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let (end, length) = loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    break (request.len(), length);
                }
                assert!(request.len() < 8192);
            };
            request.resize(end + length, 0);
            stream.read_exact(&mut request[end..]).unwrap();
            if length != 0 {
                bodies.push(request[end..].to_vec());
            }
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
        }
        assert_eq!(bodies.len(), 9);
        assert!(bodies.iter().all(|body| body == &bodies[0]));
    });
    // This integration test runs in its own process; credentials are synthetic.
    unsafe {
        std::env::set_var("HORIZON_API_DISCOVERY", format!("{origin}/discovery"));
    }
    horizon_protocol::vfp_core::configure(horizon_protocol::vfp_core::Config {
        enabled: true,
        ..Default::default()
    });
    let session = AuthSession::new("Fixture", 42, "synthetic-test-token");
    assert_eq!(auth::join_server(&session, "test-server-hash"), Ok(()));
    assert_eq!(
        auth::join_server(&session, "test-server-hash"),
        Err(JoinError::InvalidSession)
    );
    assert_eq!(
        auth::join_server(&session, "test-server-hash"),
        Err(JoinError::Unavailable)
    );
    horizon_protocol::vfp_core::configure(horizon_protocol::vfp_core::Config {
        enabled: false,
        ..Default::default()
    });
    assert_eq!(auth::join_server(&session, "test-server-hash"), Ok(()));
    server.join().unwrap();
}

#[test]
fn legacy_chunk_boundary_meshes_without_inventing_neighbor_columns() {
    let mut world =
        horizon_client_render::world::World::new(horizon_world::registry::BlockStateId(0), 0);
    world.chunks.insert(
        (0, 0),
        std::sync::Arc::new(horizon_client_render::chunk::Chunk::empty(0, 0)),
    );
    let region = vfp_render_compat::region(&world, 0, 0).unwrap();
    assert!(region[1][1].is_some());
    assert!(region[0][0].is_none());
    assert_eq!(world.chunks.len(), 1);
    assert!(vfp_render_compat::region(&world, 1, 0).is_none());
}
