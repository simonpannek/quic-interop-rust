---
title: ACN WS22/23 QUIC Project Problem 1
---

# Answers

a) Quic is described as a secure general-purpose transport protocol.
Its handshake combines the negotiation of crypthographic and transport parameters.
Building on UDP, this means QUIC mainly replaces TCP and also includes the cryptographic functionality of TLS encryption. [@RFC9000.1]

Main differences to TCP are:

- Handshake: TCP requires a three way handshake, QUIC only requires one packet [@RFC9000.7]

- Stream multiplexing: A single connection can contain multiple streams, allowing for prioritization and decoupling
of packet streams [@RFC9000.2.3]

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

f) You need to add the line

```127.0.0.1  server```

. After reloading the systemd-hostnamed service, the changes will take effect.

Domain names can be very important to QUIC, because during the handshake, the server is able to choose
where the client sends the data, for example by populating DNS records [@RFC9000.7].

g) Final configuration of the `implementation.json` file:

```
{
    "name": {
        "path": "/root/acn-quic-example"
    }
}
```

h) Stdout running all the test:


```
Saving logs to logs_2022-11-27T18:28:44
Servers: name
Clients: name
Testcases: handshake transfer multihandshake...
Measurements: goodput qlog optimize

---
1/12
Test: handshake
Server: name  Client: name
---
Test successful

---
2/12
Test: transfer
Server: name  Client: name
---
Test successful

---
3/12
Test: multihandshake
Server: name  Client: name
---
Test successful

---
4/12
Test: chacha20
Server: name  Client: name
---
Test successful

---
5/12
Test: versionnegotiation
Server: name  Client: name
---
Test successful

---
6/12
Test: transportparameter
Server: name  Client: name
---
Test successful

---
7/12
Test: retry
Server: name  Client: name
---
Test successful

---
8/12
Test: resumption
Server: name  Client: name
---
Test successful

---
9/12
Test: zerortt
Server: name  Client: name
---
Test successful

---
10/12
Measurement: goodput
Server: name
Client: name
---
Run measurement 1/1
Transferring 1073.74 MB took 11.089 s. Goodput: 774.654 Mbps
Test successful

---
11/12
Measurement: qlog
Server: name
Client: name
---
Run measurement 1/1
Transferring 209.72 MB took 2.745 s. Goodput (with qlog): 611.290 Mbps
Test successful

---
12/12
Measurement: optimize
Server: name
Client: name
---
Run measurement 1/1
Transferring 209.72 MB took 2.188 s. Goodput: 766.680 Mbps
Test successful


Run took 0:02:13.996302

↓clients/servers→
+------+------------------------+
|      |          name          |
+------+------------------------+
| name | H,T,MHS,C20,V,TP,S,R,Z |
|      |                        |
|      |                        |
+------+------------------------+
+------+---------------------------+
|      |            name           |
+------+---------------------------+
| name |  G: 774.65 (± 0.00) Mbps  |
|      |  Q: 611.29 (± 0.00) Mbps  |
|      | Opt: 766.68 (± 0.00) Mbps |
+------+---------------------------+
Exporting results to logs_2022-11-27T18:28:44/result.json
python3 run.py  20.24s user 17.73s system 28% cpu 2:14.15 total
```

Stdout running `python3 run.py -d -t handshake`:

```
Running None False
Saving logs to logs_2022-11-29T12:13:08
Servers: name
Clients: name
Testcases: handshake
Create venv: /tmp/name-server
Setup:
 . /tmp/name-server/bin/activate; ./setup-env.sh


. /tmp/name-server/bin/activate; TESTCASE=wldogo DOWNLOADS=/tmp/compliance_downloads_5db1bjtc LOGS=/tmp/logs_bcz0u3fh QLOGDIR=/tmp/logs_bcz0u3fh SSLKEYLOGFILE=/tmp/logs_bcz0u3fh/keys.log IP=localhost PORT=4433 CERTS=/tmp/compliance_certs_28otlww5 WWW=/tmp/compliance_www_r3dv2g7m ./run-server.sh
name server compliant.
Create venv: /tmp/name-client
Setup:
 . /tmp/name-client/bin/activate; ./setup-env.sh

Running with server name (/root/acn-quic-example) and client name (/root/acn-quic-example)

. /tmp/name-client/bin/activate; TESTCASE=hxnyof DOWNLOADS=/tmp/compliance_downloads_sfztrvcu LOGS=/tmp/logs_ervv8_tj QLOGDIR=/tmp/logs_ervv8_tj SSLKEYLOGFILE=/tmp/logs_ervv8_tj/keys.log IP=localhost PORT=4433 CERTS=/tmp/compliance_certs_xxmliro6 WWW=/tmp/compliance_www_ls4sh3kj ./run-client.sh
name client compliant.

---
1/1
Test: handshake
Server: name  Client: name
---
Generated random file: /tmp/www_zxd5y9qo/lopcrmodym of size: 1024
Requests: https://server:4433/lopcrmodym

Starting tcpdump on lo
Starting server:
 . /tmp/name-server/bin/activate; SSLKEYLOGFILE=/tmp/logs_server_5je22d7w/keys.log QLOGDIR=/tmp/logs_server_5je22d7w/server_qlog/ LOGS=/tmp/logs_server_5je22d7w TESTCASE=handshake WWW=/tmp/www_zxd5y9qo/ CERTS=/tmp/certs_f_xvda_q/ IP=127.0.0.1 PORT=4433 SERVERNAME=server ./run-server.sh

Starting client:
 . /tmp/name-client/bin/activate; SSLKEYLOGFILE=/tmp/logs_client_d5tpytzg/keys.log QLOGDIR=/tmp/logs_client_d5tpytzg/client_qlog/ LOGS=/tmp/logs_client_d5tpytzg TESTCASE=handshake DOWNLOADS=/tmp/download_7a2pybvi/ REQUESTS="https://server:4433/lopcrmodym" ./run-client.sh

process psutil.Process(pid=794, status='terminated', started='12:13:12') terminated with exit code None
process psutil.Process(pid=797, status='terminated', started='12:13:14') terminated with exit code None
process psutil.Process(pid=800, status='terminated', started='12:13:14') terminated with exit code None
Client stdout
client exited with code 0

tcpdump stderr
tcpdump: listening on lo, link-type EN10MB (Ethernet), snapshot length 262144 bytes
15 packets captured
30 packets received by filter
0 packets dropped by kernel

Server stderr
Terminated

Using the client's key log file.
Using selector: EpollSelector
Check of downloaded files succeeded.
Using the client's key log file.
Test successful
Test took 9.883061s


Run took 0:00:13.665528

↓clients/servers→
+------+------+
|      | name |
+------+------+
| name |  H   |
|      |      |
|      |      |
+------+------+
Exporting results to logs_2022-11-29T12:13:08/result.json
python3 run.py -d -t handshake  4.59s user 1.11s system 40% cpu 13.941 total
```

# Bibliography