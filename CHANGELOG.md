## 0.2
 * Add support for an async client returning futures
 * Split transport in `ClientTransport`, `AsyncClientTransport`, and `ServerTransport`
 * tx\_finalize now returns state, consume by rx\_response
 * Make macro-generated types (`RPCClient`, `RPCServer`) public
 * Allow retrieving the underlying channel from built-in transports
 * `JSONAsyncClientTransport` is an async version of `JSONTransport`
