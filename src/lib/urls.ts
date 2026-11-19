/**
 * The supported sites and their page URLs (the Rust side builds the same
 * ones). A service is bare on Pawchive (`patreon`) and carries its site
 * elsewhere (`patreon@kemono`), exactly as the Rust side stores it.
 */

export type SiteId = 'pawchive' | 'kemono' | 'coomer';

export const SITES: ReadonlyArray<{ id: SiteId; name: string; host: string }> = [
  { id: 'pawchive', name: 'Pawchive', host: 'pawchive.pw' },
  { id: 'kemono', name: 'Kemono', host: 'kemono.cr' },
  { id: 'coomer', name: 'Coomer', host: 'coomer.st' },
];

/** Every domain a site used; old links still open. */
const DOMAINS: Record<SiteId, readonly string[]> = {
  pawchive: ['pawchive.pw'],
  kemono: ['kemono.cr', 'kemono.su', 'kemono.party'],
  coomer: ['coomer.st', 'coomer.su', 'coomer.party'],
};

/** True when the text mentions a supported site (for paste and drop). */
export function mentionsSite(text: string): boolean {
  return /(?:pawchive\.pw|kemono\.(?:cr|su|party)|coomer\.(?:st|su|party))\//i.test(text);
}

export function siteOf(host: string): SiteId | null {
  const h = host.toLowerCase().replace(/\.$/, '');
  for (const [id, domains] of Object.entries(DOMAINS) as Array<[SiteId, readonly string[]]>) {
    if (domains.some((d) => h === d || h.endsWith(`.${d}`))) return id;
  }
  return null;
}

export function splitService(service: string): { site: SiteId; service: string } {
  const at = service.lastIndexOf('@');
  if (at < 0) return { site: 'pawchive', service };
  const site = service.slice(at + 1) as SiteId;
  return {
    site: SITES.some((s) => s.id === site) ? site : 'pawchive',
    service: service.slice(0, at),
  };
}

export function siteName(site: SiteId): string {
  return SITES.find((s) => s.id === site)?.name ?? 'Pawchive';
}

export function siteOrigin(site: SiteId): string {
  return `https://${SITES.find((s) => s.id === site)?.host ?? 'pawchive.pw'}`;
}

/** "patreon" or "patreon on Kemono", for showing a service to people. */
export function serviceLabel(service: string): string {
  const s = splitService(service);
  return s.site === 'pawchive' ? s.service : `${s.service} / ${siteName(s.site)}`;
}

const e = encodeURIComponent;

/** The page a download came from: its post, else its creator. */
export function browseUrlOf(
  creator: { service: string; id: string } | null | undefined,
  refs: ReadonlyArray<{ service: string; creatorId: string; postId: string }>,
): string | null {
  const [only] = refs;
  if (refs.length === 1 && only) return pageUrls.post(only.service, only.creatorId, only.postId);
  if (creator) return pageUrls.creator(creator.service, creator.id);
  const first = refs[0];
  if (first && refs.every((r) => r.service === first.service && r.creatorId === first.creatorId)) {
    return pageUrls.creator(first.service, first.creatorId);
  }
  return null;
}

export const pageUrls = {
  post: (service: string, creatorId: string, postId: string) => {
    const s = splitService(service);
    return `${siteOrigin(s.site)}/${e(s.service)}/user/${e(creatorId)}/post/${e(postId)}`;
  },
  creator: (service: string, creatorId: string) => {
    const s = splitService(service);
    return `${siteOrigin(s.site)}/${e(s.service)}/user/${e(creatorId)}`;
  },
  icon: (service: string, creatorId: string) => {
    const s = splitService(service);
    const origin =
      s.site === 'pawchive' ? siteOrigin(s.site) : `https://img.${siteOrigin(s.site).slice(8)}`;
    return `${origin}/icons/${e(s.service)}/${e(creatorId)}`;
  },
  recent: (site: SiteId) => `${siteOrigin(site)}/posts`,
};
