## 0.2
 * Add support for an async client returning futures
 * Split transport into `ClientTransport`, `AsyncClientTransport`, and `ServerTransport`
 * `tx_finalize` consumes the `TXState`
 * `tx_finalize` now returns state, consume by rx\_response
 * Make macro-generated types (`RPCClient`, `RPCServer`) public
 * Allow retrieving the underlying channel from built-in transports
 * `JSONAsyncClientTransport` is an async version of `JSONTransport`
 * `BincodeAsyncClientTransport` is an async version of `BincodeTransport`
