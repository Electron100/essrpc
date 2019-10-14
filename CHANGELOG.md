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
