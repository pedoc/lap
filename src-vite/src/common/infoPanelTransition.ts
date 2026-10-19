// Shared height/opacity animation for the information panel's collapsible sections.
export function onBeforeEnter(el: Element) {
  const element = el as HTMLElement;
  element.style.opacity = '0';
  element.style.height = '0';
}

export function onEnter(el: Element) {
  const element = el as HTMLElement;
  element.style.transition = 'all 0.1s ease';
  element.style.height = `${element.scrollHeight}px`;
  element.style.opacity = '1';
}

export function onAfterEnter(el: Element) {
  const element = el as HTMLElement;
  element.style.height = '';
}

export function onLeave(el: Element) {
  const element = el as HTMLElement;
  element.style.transition = 'all 0.1s ease';
  element.style.height = `${element.scrollHeight}px`;
  void element.offsetHeight;
  element.style.height = '0';
  element.style.opacity = '0';
}
