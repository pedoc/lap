import aliases from '../assets/location-aliases-zh.json' with {type: 'json'};

const chinese = locale => String(locale || '').toLowerCase().startsWith('zh');
export function displayRegionName(cc, region, locale) {
  if (!chinese(locale) || !region) return region || '';
  return aliases.regions[`${cc}|${region}`] || region;
}
export function displayPlaceName(cc, region, name, locale) {
  if (!chinese(locale) || !name) return name || '';
  return aliases.places[`${cc}|${region}|${name}`] || name;
}

// Stored names remain stable SQL filter keys. Sort display labels and their parallel counts together.
export function localizedLocationGroups(groups, locale, sort = 0) {
  const collator = new Intl.Collator(locale || 'en', {numeric: true, sensitivity: 'base'});
  const order = (a, b) => sort === 2 ? a.count - b.count || collator.compare(a.label, b.label)
    : sort === 3 ? b.count - a.count || collator.compare(a.label, b.label)
    : (sort === 1 ? -1 : 1) * collator.compare(a.label, b.label);
  const result = groups.map(group => {
    const items = group.names.map((name, index) => ({name, label: displayPlaceName(group.cc, group.admin1, name, locale), count: group.counts[index]})).sort(order);
    return {...group, displayAdmin1: displayRegionName(group.cc, group.admin1, locale),
      names: items.map(item => item.name), displayNames: items.map(item => item.label), counts: items.map(item => item.count)};
  });
  return result.sort((a, b) => order({label:a.displayAdmin1, count:a.counts.reduce((sum,n)=>sum+n,0)}, {label:b.displayAdmin1, count:b.counts.reduce((sum,n)=>sum+n,0)}));
}
export function filterLocationGroups(groups, query, countryName) {
  const q = query.trim().toLowerCase();
  if (!q) return groups;
  const includes = value => String(value || '').toLowerCase().includes(q);
  return groups.flatMap(group => {
    if ([group.admin1, group.displayAdmin1, countryName(group.cc)].some(includes)) return [group];
    const indexes = group.names.map((name,index) => includes(name) || includes(group.displayNames[index]) ? index : -1).filter(index => index >= 0);
    return indexes.length ? [{...group, names:indexes.map(i=>group.names[i]), displayNames:indexes.map(i=>group.displayNames[i]), counts:indexes.map(i=>group.counts[i])}] : [];
  });
}

export function displayLocationFilterLabel(selection, locale) {
  return [displayRegionName(selection.cc, selection.admin1, locale),
    displayPlaceName(selection.cc, selection.admin1, selection.name, locale)].filter(Boolean).join(' > ');
}
