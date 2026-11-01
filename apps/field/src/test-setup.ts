import '@testing-library/jest-dom/vitest'
import { configure } from '@testing-library/react'

// findBy*/waitFor default to 1s; lazy routes miss it when turbo runs several suites at once.
configure({ asyncUtilTimeout: 5_000 })
