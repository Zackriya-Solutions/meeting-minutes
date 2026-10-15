'use client';

import { createContext } from 'react';
import type { Person } from '@/types';

/**
 * Known people for the name autocomplete. A context, so a refresh of the list reaches the open
 * name form without re-rendering the transcript rows.
 */
export const PeopleContext = createContext<Person[]>([]);
