// Official SDKs are shared per WebView, but each viewport owns a native map instance.
const scripts = new Map();
const providers = new Map();
function loadScript(url, ready) {
  if (ready()) return Promise.resolve();
  if (scripts.has(url)) return scripts.get(url).then(() => {if (ready()) return; scripts.delete(url); return loadScript(url,ready);});
  const promise = new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.charset = 'utf-8'; script.src = url;
    const fail = message => {clearTimeout(timer); script.remove(); reject(new Error(message));};
    const timer = setTimeout(() => fail('Map SDK script download timed out; check proxy and connectivity'),30000);
    script.onload = () => {clearTimeout(timer); ready() ? resolve() : fail('Map SDK did not initialize');};
    script.onerror = () => fail('Map SDK script could not be downloaded; check proxy and connectivity');
    document.head.appendChild(script);
  }).catch(error => {scripts.delete(url); throw error;});
  scripts.set(url, promise); return promise;
}
export function loadMapSdk(provider, config) {
  if (!config.jsKey || (provider === 'amap' && !config.securityJsCode)) return Promise.reject(new Error('JavaScript key/security code is missing'));
  const key = JSON.stringify([config.jsKey, config.securityJsCode]);
  const previous = providers.get(provider);
  if (previous?.key === key) return previous.promise;
  // Serialize key changes: a late old loader must never overwrite the new SDK globals.
  const promise = Promise.resolve(previous?.promise).catch(() => {}).then(async () => {
    if (provider === 'amap') {
      await loadScript('https://webapi.amap.com/loader.js', () => !!window.AMapLoader);
      window.AMapLoader.reset?.();
      window._AMapSecurityConfig = {securityJsCode: config.securityJsCode};
      return window.AMapLoader.load({key: config.jsKey, version: '2.0', plugins: ['AMap.Scale', 'AMap.ControlBar']});
    }
    if (provider === 'tencent') {
      if (previous) delete window.TMap;
      const url = `https://map.qq.com/api/gljs?v=1.exp&key=${encodeURIComponent(config.jsKey)}`;
      await loadScript(url, () => !!window.TMap); return window.TMap;
    }
    throw new Error('Unsupported native map SDK');
  }).catch(() => {
    if (providers.get(provider)?.promise === promise) providers.delete(provider);
    throw new Error(`${provider} SDK initialization failed; check JavaScript key, allowed domains, security code and proxy`);
  });
  providers.set(provider, {key, promise}); return promise;
}
