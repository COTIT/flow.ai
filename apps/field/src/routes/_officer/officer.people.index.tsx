import { createFileRoute } from '@tanstack/react-router'

import { PeopleScreen } from '@/features/officer/PeopleScreen'
import { validatePeopleSearch } from '@/features/officer/peopleSearch'

export const Route = createFileRoute('/_officer/officer/people/')({
  // Filter state lives in the URL (spec §10). The screen validates again at
  // the point of use: useSearch() returns raw params in this router version.
  validateSearch: validatePeopleSearch,
  component: PeopleScreen,
})
