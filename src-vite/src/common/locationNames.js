import { hasResolvableGps } from './mapServices.js';
/** Address labels always come from persisted provider data, never a transliteration dictionary. */
export function addressForFile(file) {
  return hasResolvableGps(file?.gps_latitude, file?.gps_longitude) ? file?.location : null;
}
export function addressPath(address) {
  return (address?.hierarchy || []).map(level => level.name).filter(Boolean).join(' > ');
}
export function addressLabel(address) {
  return address?.address || addressPath(address);
}
export function addressSelection(address) {
  const levels = address?.hierarchy || [];
  return {nodeId: levels.at(-1)?.nodeId || null, label: addressPath(address), path: levels.map(level => ({nodeId:level.nodeId,name:level.name}))};
}
export function displayLocationFilterLabel(selection) {
  return selection?.label || '';
}
export function locationNodeChain(node, nodes) {
  const byId = nodes instanceof Map ? nodes : new Map(nodes.map(item => [item.id, item]));
  const names = [], visited = new Set();
  for (let current = node; current && !visited.has(current.id); current = byId.get(current.parentId)) {
    visited.add(current.id); names.unshift({nodeId:current.id,name:current.name});
  }
  return names;
}
export function locationNodePath(node, nodes) {
  return locationNodeChain(node,nodes).map(level => level.name).join(' > ');
}
/** Flat visible rows retain arbitrary depth, ancestors of search hits and stable node IDs. */
export function visibleLocationRows(nodes, expanded = new Set(), search = '', sort = 0) {
  const children = new Map(), ids = new Set(nodes.map(node => node.id));
  const byId = new Map(nodes.map(node => [node.id,node]));
  for (const node of nodes) {
    const parent = ids.has(node.parentId) ? node.parentId : null;
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent).push(node);
  }
  const needle = search.trim().toLocaleLowerCase();
  const rows = [], visiting = new Set();
  function walk(parent, depth) {
    const items = children.get(parent) || [];
    items.sort((a,b) => {
      const name = a.name.localeCompare(b.name);
      return sort === 1 ? -name : sort === 2 ? a.count-b.count || name : sort === 3 ? b.count-a.count || name : name;
    });
    for (const node of items) {
      if (visiting.has(node.id)) continue;
      const label = locationNodePath(node, byId);
      const matches = !needle || label.toLocaleLowerCase().includes(needle);
      const start = rows.length;
      visiting.add(node.id);
      rows.push({...node, label, depth, hasChildren: children.has(node.id)});
      if (needle || expanded.has(node.id)) walk(node.id, depth + 1);
      visiting.delete(node.id);
      if (needle && !matches && rows.length === start + 1) rows.pop();
    }
  }
  walk(null, 0);
  return rows;
}
