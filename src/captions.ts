import type { Caption, EngineStatus } from './types';

export interface VisibleCaption extends Caption { expires: number }
export function appendCaption(current: VisibleCaption[], caption: Caption, status: EngineStatus, now: number): VisibleCaption[] {
  if (!status.running || caption.generation !== status.generation) return current;
  if (current.some(c => c.generation === caption.generation && c.id === caption.id)) return current;
  return [...current.filter(c => c.expires > now), { ...caption, expires: now + 8000 }].slice(-3);
}
