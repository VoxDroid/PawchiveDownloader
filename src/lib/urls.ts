/** Pawchive page URLs (the Rust side builds the same ones). */
const ORIGIN = 'https://pawchive.pw';
const e = encodeURIComponent;

export const pageUrls = {
  post: (service: string, creatorId: string, postId: string) =>
    `${ORIGIN}/${e(service)}/user/${e(creatorId)}/post/${e(postId)}`,
  creator: (service: string, creatorId: string) => `${ORIGIN}/${e(service)}/user/${e(creatorId)}`,
  icon: (service: string, creatorId: string) => `${ORIGIN}/icons/${e(service)}/${e(creatorId)}`,
};
