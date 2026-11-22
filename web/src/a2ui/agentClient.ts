export async function fetchA2uiMessagesFromEndpoint(url: string): Promise<any[]> {
  const res = await fetch(url, {
    method: 'GET',
    headers: { Accept: 'application/json, text/event-stream, application/x-ndjson' },
  });

  if (!res.ok) {
    throw new Error(`Agent endpoint returned HTTP ${res.status}`);
  }

  const contentType = res.headers.get('content-type') ?? '';
  const text = await res.text();

  if (contentType.includes('application/json') || text.trim().startsWith('[') || text.trim().startsWith('{')) {
    const parsed = JSON.parse(text);
    return Array.isArray(parsed) ? parsed : [parsed];
  }

  // Support SSE (`data: {...}`) or NDJSON line-delimited A2UI messages
  const messages: any[] = [];
  for (const rawLine of text.split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith(':')) continue;
    const payload = line.startsWith('data:') ? line.slice(5).trim() : line;
    if (!payload || payload === '[DONE]') continue;
    try {
      messages.push(JSON.parse(payload));
    } catch {
      // skip non-JSON SSE lines
    }
  }
  return messages;
}
