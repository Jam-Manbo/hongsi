import DOMPurify from 'dompurify';

DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    node.setAttribute('target', '_blank');
    node.setAttribute('rel', 'noopener noreferrer');
  }
});

export function cleanHtml(html: string | null | undefined): string {
  return DOMPurify.sanitize(html ?? '', {
    FORBID_TAGS: ['style', 'form', 'input', 'iframe', 'script'],
    FORBID_ATTR: ['style'],
  });
}
