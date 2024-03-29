# QUIC project

This project was written as part of the Advanced Computer Networking lecture at TUM.

HTTP3 client and server written in Rust, using [rustls](https://github.com/rustls/rustls) for encryption, [quinn](https://github.com/quinn-rs/quinn) for the QUIC implementation, and [h3](https://github.com/hyperium/h3) for the HTTP/3 implementation. The code is to some part inspired from the h3 [client](https://github.com/hyperium/h3/blob/master/examples/client.rs) and [server](https://github.com/hyperium/h3/blob/master/examples/server.rs) examples.

## Why quinn?
In the previous milestone of this project, I opted for using [quiche](https://github.com/cloudflare/quiche) as a QUIC implementation. When I started to work on the second milestone, I quickly noticed that quiche is implemented on a far lower level than required: For instance, quiche requires the user to manually read from UDP sockets, before giving the received packets to the library. It also doesn't feature an async implementation, so the user is responsible to create some kind of event loop to handle events.
This is much more granular than required for the scope of this project. This is why I decided to use quinn instead. Quinn also implements the QUIC transport protocol, and already includes a high-level async API based on [tokio](https://github.com/tokio-rs/tokio). The h3 library directly builds on top of quinn, and according to the h3 project description, both quinn and h3 are tested for interoperability and performance in the quic-interop-runner, making those libraries a suitable choice for this project.
