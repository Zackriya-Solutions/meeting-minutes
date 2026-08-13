import catalogData from '../../showcase.catalog.json';

export type ShowcaseItem = {
  id: string;
  title: string;
  source: string;
  productionRoot?: string;
  kind: 'foundation' | 'primitive' | 'product';
  states?: string[];
  boundaries?: string[];
};

export type ShowcaseGroup = {
  id: string;
  title: string;
  items: ShowcaseItem[];
};

export const showcaseCatalog = catalogData as {
  title: string;
  groups: ShowcaseGroup[];
};

export const showcaseItems = showcaseCatalog.groups.flatMap((group) => group.items);
