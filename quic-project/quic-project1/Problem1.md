---
title: ACN WS22/23 QUIC Project Problem 1
---

# Answers

a) Quic is described as a secure general-purpose transport protocol.
Its handshake combines the negotiation of crypthographic and transport parameters.
Building on UDP, this means QUIC mainly replaces TCP and also includes the cryptographic functionality of TLS encryption. [@RFC9000.1]

Main differences to TCP are... [TODO]

b) QUIC communication works by exchanging encrypted QUIC packets. Those packets usually contain QUIC frames, which carry control
information and application data. [@RFC9000.1]

c) Transport parameters are shared when establishing a connection. Those parameters set certain restrictions and the
endpoints are required to comply with them. [@RFC9000.7.4]

The following transport parameters are available: original_destination_connection_id, max_idle_timeout, stateless_reset_token,
max_udp_payload_size, initial_max_data, initial_max_stream_data_bidi_local, initial_max_stream_data_bidi_remote,
initial_max_stream_data_uni, initial_max_streams_bidi, initial_max_streams_uni, ack_delay_exponent, max_ack_delay,
disable_active_migration, preferred_address, active_connection_id_limit, initial_source_connection_id, retry_source_connection_id [@RFC9000.18.2]

d) I want to use quiche [@QUICHE] for the remainder of the project. The reason for this is that I really like to work with
Rust, especially in networking context, as it's very performant and I feel much safer when it comes to memory management.
When looking at different Rust imeplementations, quiche looked like the most complete and well-maintained library.

e) Before sending actual data, the connection first needs to be established during a handshake phase. During this phase,
the client first sends an  initial CRYPTO frame. Then, the server responds, acknowledging the received packet and responding with a handshake. This packet is
again acknowledged, the final crypto package is sent to the server and the client can start sending data, for example using
a stream frame. With the next response, the server still has to send a HANDSHAKE_DONE frame, confirming that the handshake
is done to the client. [@RFC9000.7]

All data sent using QUIC is encrypted before sending. As a link can only support packets of a certain length and UDP doesn't
guarantee that the send and receive order match, some data might need to get split into multiple packets and then arrives
at the other endpoint in an incorrect order. This means, the implementation needs to maintain a buffer of a size of at least
the advertised flow control limit, reordering and decrypting the data, even if received out of order. [@RFC9000.2.2]

f) You need to add the line `127.0.0.1  server`. After reloading the systemd-hostnamed service, the changes will take effect.

Domain names can be very important to QUIC, because [TODO]

g) Final configuration of the `implementation.json` file:

```
{
    "name": {
        "path": "/root/acn-quic-example"
    }
}
```

h) Content of the `result.json` file:


```
{
  "interop_commit_hash": "62dbae60e457772d2eb64b1185e84c78e6e04c5f",
  "interop_start_time_unix_timestamp": 1669222451.357238,
  "interop_end_time_unix_timestamp": 1669222589.495605,
  "log_dir": "logs_2022-11-23T17:54:11",
  "server_node_name": null,
  "client_node_name": null,
  "node_image": null,
  "server_implementations": {
    "name": "f2f6477638a27386a9f61af8a8dc8094d52c7caf"
  },
  "client_implementations": {
    "name": "f2f6477638a27386a9f61af8a8dc8094d52c7caf"
  },
  "bandwidth_limit": "None",
  "tests": {
    "H": {
      "name": "handshake",
      "desc": "Handshake completes successfully."
    },
    "T": {
      "name": "transfer",
      "desc": "Stream data is being sent and received correctly. Connection close completes with a zero error code."
    },
    "MHS": {
      "name": "multihandshake",
      "desc": "Stream data is being sent and received correctly. Connection close completes with a zero error code."
    },
    "C20": {
      "name": "chacha20",
      "desc": "Handshake completes using ChaCha20."
    },
    "V": {
      "name": "versionnegotiation",
      "desc": "A version negotiation packet is elicited and acted on."
    },
    "TP": {
      "name": "transportparameter",
      "desc": "Hundreds of files are transferred over a single connection, and server increased stream limits to accommodate client requests."
    },
    "S": {
      "name": "retry",
      "desc": "Server sends a Retry, and a subsequent connection using the Retry token completes successfully."
    },
    "R": {
      "name": "resumption",
      "desc": "Connection is established using TLS Session Resumption."
    },
    "Z": {
      "name": "zerortt",
      "desc": "0-RTT data is being sent and acted on."
    },
    "G": {
      "name": "goodput",
      "desc": "Measures connection goodput as baseline."
    },
    "Q": {
      "name": "qlog",
      "desc": "Measures connection goodput while running qlog."
    },
    "Opt": {
      "name": "optimize",
      "desc": "Measures connection goodput with optimizations."
    }
  },
  "quic_draft": 34,
  "quic_version": "0x1",
  "results": [
    [
      {
        "abbr": "H",
        "name": "handshake",
        "result": "succeeded"
      },
      {
        "abbr": "T",
        "name": "transfer",
        "result": "succeeded"
      },
      {
        "abbr": "MHS",
        "name": "multihandshake",
        "result": "succeeded"
      },
      {
        "abbr": "C20",
        "name": "chacha20",
        "result": "succeeded"
      },
      {
        "abbr": "V",
        "name": "versionnegotiation",
        "result": "succeeded"
      },
      {
        "abbr": "TP",
        "name": "transportparameter",
        "result": "succeeded"
      },
      {
        "abbr": "S",
        "name": "retry",
        "result": "succeeded"
      },
      {
        "abbr": "R",
        "name": "resumption",
        "result": "succeeded"
      },
      {
        "abbr": "Z",
        "name": "zerortt",
        "result": "succeeded"
      }
    ]
  ],
  "measurements": [
    [
      {
        "name": "goodput",
        "abbr": "G",
        "filesize": 1073741824,
        "result": "succeeded",
        "average": "760.05 (\\u00b1 0.00) Mbps",
        "details": [
          760.0521504153775
        ],
        "server": "name",
        "client": "name"
      },
      {
        "name": "qlog",
        "abbr": "Q",
        "filesize": 209715200,
        "result": "succeeded",
        "average": "522.38 (\\u00b1 0.00) Mbps",
        "details": [
          522.3782178840545
        ],
        "server": "name",
        "client": "name"
      },
      {
        "name": "optimize",
        "abbr": "Opt",
        "filesize": 209715200,
        "result": "succeeded",
        "average": "655.00 (\\u00b1 0.00) Mbps",
        "details": [
          655.0020516132005
        ],
        "server": "name",
        "client": "name"
      }
    ]
  ]
}
```

# Bibliography
