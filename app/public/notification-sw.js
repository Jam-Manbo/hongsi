self.addEventListener('push', (event) => {
  let payload;
  try { payload = event.data?.json(); } catch { return; }
  if (!payload || typeof payload.title !== 'string' || payload.intent?.version !== 1 || !['item', 'todo', 'seat', 'notices'].includes(payload.intent?.target?.kind)) return;
  event.waitUntil(self.registration.showNotification(payload.title, {
    body: payload.body, icon: '/favicon.svg', tag: payload.tag,
    data: { intent: payload.intent },
  }));
});
self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const intent = event.notification.data?.intent;
  if (!intent || !['item', 'todo', 'seat', 'notices'].includes(intent.target?.kind)) return;
  event.waitUntil((async () => {
    const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true });
    const client = windows.find((w) => new URL(w.url).origin === self.location.origin);
    if (client) { client.postMessage({ type: 'notification-click', intent }); await client.focus(); }
    else await self.clients.openWindow('/#/home?notification=' + encodeURIComponent(JSON.stringify(intent)));
  })());
});
