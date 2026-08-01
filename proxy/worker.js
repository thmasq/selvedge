import init, { handle_push, rewrite_proxy_url } from './pkg/selvedge_proxy.js';

/// URL of the currently configured proxy server.
///
/// This value is set by the application via a `SET_PROXY_SERVER` message.
/// Requests matching `PROXY_PREFIX` will only be intercepted once this
/// variable has been initialized.
let configuredProxyServer = null;

/// Initialize the WebAssembly module during installation and immediately
/// activate this service worker without waiting for older versions to exit.
self.addEventListener('install', (event) => {
    event.waitUntil(init().then(() => self.skipWaiting()));
});

/// Take control of all existing clients immediately after activation so that
/// fetch interception and push handling become available without requiring
/// the page to be reloaded.
self.addEventListener('activate', (event) => {
    event.waitUntil(self.clients.claim());
});

/// Receives configuration messages from the application.
///
/// Supported messages:
///
/// ```js
/// {
///     type: "SET_PROXY_SERVER",
///     url: "https://proxy.example.com"
/// }
/// ```
///
/// The supplied URL becomes the base proxy server used to rewrite outgoing
/// proxied requests.
self.addEventListener('message', (event) => {
    if (event.data && event.data.type === 'SET_PROXY_SERVER') {
        configuredProxyServer = event.data.url;
    }
});

/// Forward Push API events to the WebAssembly implementation.
///
/// All push payload parsing and notification generation is delegated to the
/// Rust `handle_push()` function.
self.addEventListener('push', (event) => {
    event.waitUntil(handle_push(event));
});

/// Focus an existing application window when a notification is clicked.
///
/// If no window is currently open, a new one is created at the application
/// root (`/`).
self.addEventListener('notificationclick', (event) => {
    event.notification.close();

    event.waitUntil(
        clients.matchAll({ type: 'window' }).then((clientList) => {
            for (const client of clientList) {
                if (client.url.includes('/')) {
                    return client.focus();
                }
            }

            return clients.openWindow('/');
        })
    );
});

/// URL prefix identifying requests that should be routed through the proxy.
///
/// A request such as
///
///     /proxy-request/https://example.com/api?a=1
///
/// is rewritten into a request targeting the configured proxy server.
const PROXY_PREFIX = "/proxy-request/";

/// Intercepts requests beginning with `PROXY_PREFIX` and transparently routes
/// them through the configured proxy server.
///
/// The intercepted URL is reconstructed, passed through the Rust
/// `rewrite_proxy_url()` helper, and then fetched while preserving the
/// original HTTP method, headers, credentials, and body.
///
/// Navigation requests are forced to use `"cors"` mode because browsers do
/// not allow service workers to synthesize `"navigate"` fetches.
self.addEventListener("fetch", (event) => {
    // Proxying is disabled until the application configures a proxy server.
    if (!configuredProxyServer) return;

    const requestUrl = new URL(event.request.url);

    // Ignore requests that are not intended for proxying.
    if (!requestUrl.pathname.startsWith(PROXY_PREFIX)) {
        return;
    }

    // Recover the original destination URL from the request path.
    const targetUrl =
        requestUrl.pathname.slice(PROXY_PREFIX.length) + requestUrl.search;

    // Convert the target URL into a proxy endpoint.
    const newUrl = rewrite_proxy_url(targetUrl, configuredProxyServer);

    event.respondWith(
        fetch(newUrl, {
            // "navigate" cannot be used when constructing a fetch request.
            mode: event.request.mode === "navigate" ? "cors" : event.request.mode,
            credentials: event.request.credentials,
            headers: event.request.headers,
            method: event.request.method,
            body: event.request.body,
        }).catch((error) => {
            console.error(`Failed to make proxied request to ${targetUrl}:`, error);
            throw error;
        })
    );
});
