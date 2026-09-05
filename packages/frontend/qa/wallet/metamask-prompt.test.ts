import { describe, expect, it } from 'vitest';
import { isMetaMaskPromptUrl, metamaskNotificationUrl } from './metamask-prompt';

const EXT = 'gadekpdjmpjjnnemgnhkbjgnjpdaakgh';

describe('isMetaMaskPromptUrl', () => {
  it('treats MetaMask notification.html as a prompt', () => {
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/notification.html`)).toBe(true);
  });

  it('treats notification.html with a query string as a prompt', () => {
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/notification.html?tabId=1`)).toBe(true);
  });

  it('treats popup.html as a prompt', () => {
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/popup.html`)).toBe(true);
  });

  it('treats home.html with a request query as a prompt', () => {
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/home.html?id=1`)).toBe(true);
  });

  it('treats home.html hashes that name a confirmation as a prompt', () => {
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/home.html#connect`)).toBe(true);
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/home.html#confirmation`)).toBe(true);
  });

  it('does not treat a DApp tab as a prompt', () => {
    expect(isMetaMaskPromptUrl('https://chakra-ag.vercel.app/')).toBe(false);
  });

  it('does not treat MetaMask home without a request as a prompt', () => {
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/home.html`)).toBe(false);
    expect(isMetaMaskPromptUrl(`chrome-extension://${EXT}/home.html#/`)).toBe(false);
  });
});

describe('metamaskNotificationUrl', () => {
  it('points at dappwright’s remapped MetaMask extension id', () => {
    expect(metamaskNotificationUrl()).toBe(`chrome-extension://${EXT}/notification.html`);
  });
});
