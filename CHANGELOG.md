## 0.4.1
  * Remove artificial frame size limit for Bincode transport.
## 0.4
  * Restructure async clients to match more modern paradigms. New
    dependence on Tokio (when the `async` feature is enabled).
## 0.3.1
  * Bincode and JSON transports flush the underlying channel when transmitting
## 0.3
  * Upgrade to futures-rs 0.3 and std::Future
  * Upgrade proc-macro2/syn/quote to 1.0
## 0.2.2
 * Fix serde derive dependency: Use derive feature of serde crate.

## 0.2.1
 * Add RPCErrorKind::TransportEOF, intended to be used by a Transport
   when a client disconnects (likely detected through reading an EOF)

## 0.2
 * Add support for an async client returning futures
 * Split transport into `ClientTransport`, `AsyncClientTransport`, and `ServerTransport`
 * `tx_finalize` consumes the `TXState`
 * `tx_finalize` now returns state, consume by rx\_response
 * Make macro-generated types (`RPCClient`, `RPCServer`) public
 * Allow retrieving the underlying channel from built-in transports
 * `JSONAsyncClientTransport` is an async version of `JSONTransport`
 * `BincodeAsyncClientTransport` is an async version of `BincodeTransport`
