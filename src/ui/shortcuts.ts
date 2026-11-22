export type ShortcutActions = Record<'run' | 'pause' | 'step' | 'reset' | 'open', () => void>;

export function handleShortcut(event: KeyboardEvent, actions: ShortcutActions): boolean {
  const target = event.target as HTMLElement | null;
  if (event.repeat || event.isComposing || event.altKey || event.metaKey || target?.isContentEditable ||
    /^(INPUT|TEXTAREA|SELECT)$/.test(target?.tagName ?? '')) return false;

  let action: keyof ShortcutActions;
  if (event.key === 'F5' && !(event.ctrlKey && event.shiftKey)) {
    action = event.ctrlKey ? 'reset' : event.shiftKey ? 'pause' : 'run';
  } else if (!event.ctrlKey && !event.shiftKey && event.key === 'Escape') {
    action = 'pause';
  } else if (!event.ctrlKey && !event.shiftKey && event.key === 'F10') {
    action = 'step';
  } else if (event.ctrlKey && !event.shiftKey && event.key.toLowerCase() === 'o') {
    action = 'open';
  } else return false;

  event.preventDefault();
  actions[action]();
  return true;
}
