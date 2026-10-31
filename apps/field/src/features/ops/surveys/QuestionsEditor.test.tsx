import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const saveMutateAsync = vi.fn()
const saveState = { isPending: false, isError: false, isSuccess: false, error: null as Error | null }
const removeMutate = vi.fn()
const removeState = { isPending: false, isError: false, error: null as Error | null }
const swapMutate = vi.fn()
const swapState = { isPending: false, isError: false, error: null as Error | null }

vi.mock('@/features/ops/surveys/useSurveyAdmin', () => ({
  useSaveQuestion: () => ({ mutateAsync: saveMutateAsync, reset: vi.fn(), ...saveState }),
  useRemoveQuestion: () => ({ mutate: removeMutate, reset: vi.fn(), ...removeState }),
  useSwapQuestions: () => ({ mutate: swapMutate, reset: vi.fn(), ...swapState }),
}))

const { QuestionsEditor, QuestionList } = await import('@/features/ops/surveys/QuestionsEditor')
await import('@/i18n')

const SURVEY = '30000000-0000-4000-8000-000000000001'

const question = (over: Record<string, unknown> = {}) => ({
  id: 'q1',
  survey_id: SURVEY,
  position: 1,
  kind: 'single_choice' as const,
  prompt_en: 'What is your main crop?',
  prompt_sw: null,
  required: true,
  options: [
    { value: 'maize', label_en: 'Maize', label_sw: 'Mahindi' },
    { value: 'beans', label_en: 'Beans' },
  ],
  deleted_at: null,
  created_at: '2026-09-28T08:00:00Z',
  updated_at: '2026-09-28T08:00:00Z',
  ...over,
})

const two = () => [
  question(),
  question({ id: 'q2', position: 2, kind: 'number', prompt_en: 'How many acres?', options: [] }),
]

async function chooseKind(card: HTMLElement, label: string) {
  await userEvent.click(within(card).getByTestId('question-kind'))
  await userEvent.click(await screen.findByRole('option', { name: label }))
}

beforeEach(() => {
  saveMutateAsync.mockReset()
  saveMutateAsync.mockResolvedValue({ id: 'q9' })
  removeMutate.mockReset()
  swapMutate.mockReset()
  Object.assign(saveState, { isPending: false, isError: false, isSuccess: false, error: null })
  Object.assign(removeState, { isPending: false, isError: false, error: null })
  Object.assign(swapState, { isPending: false, isError: false, error: null })
})

describe('the question list', () => {
  test('no questions says so and offers to add one', () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={[]} />)

    expect(screen.getByText('No questions yet.')).toBeInTheDocument()
    expect(screen.getByTestId('question-add')).toHaveTextContent('Add question')
  })

  test('shows each question in the order the database returned', () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)

    const cards = screen.getAllByTestId('question-card')
    expect(cards).toHaveLength(2)
    expect(within(cards[0]).getByTestId('question-prompt-en')).toHaveValue('What is your main crop?')
    expect(within(cards[1]).getByTestId('question-prompt-en')).toHaveValue('How many acres?')
  })

  test('a choice question shows its options; a number question has none', () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)

    const [choice, number] = screen.getAllByTestId('question-card')
    expect(within(choice).getAllByTestId('question-option')).toHaveLength(2)
    expect(within(choice).getAllByTestId('question-option-label-sw')[0]).toHaveValue('Mahindi')
    expect(within(number).queryByTestId('question-option')).not.toBeInTheDocument()
    expect(within(number).queryByTestId('question-add-option')).not.toBeInTheDocument()
  })
})

describe('adding a question', () => {
  test('saves after the last position, with no id', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    await userEvent.click(screen.getByTestId('question-add'))

    const card = screen.getByTestId('question-card-new')
    await userEvent.type(within(card).getByTestId('question-prompt-en'), 'Do you irrigate?')
    await chooseKind(card, 'Yes or no')
    await userEvent.click(within(card).getByTestId('question-save'))

    expect(saveMutateAsync).toHaveBeenCalledWith({
      position: 3,
      kind: 'yes_no',
      prompt_en: 'Do you irrigate?',
      prompt_sw: '',
      required: true,
      options: [],
    })
    // Saved: the new card closes and the refetched list shows the question.
    await waitFor(() => expect(screen.queryByTestId('question-card-new')).not.toBeInTheDocument())
  })

  test('the first question goes at position 1', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={[]} />)
    await userEvent.click(screen.getByTestId('question-add'))

    const card = screen.getByTestId('question-card-new')
    await userEvent.type(within(card).getByTestId('question-prompt-en'), 'Main crop?')
    await userEvent.click(within(card).getByTestId('question-save'))

    expect(saveMutateAsync.mock.calls[0][0]).toMatchObject({ position: 1 })
  })

  test('an unsaved new question can be discarded', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={[]} />)
    await userEvent.click(screen.getByTestId('question-add'))
    await userEvent.click(within(screen.getByTestId('question-card-new')).getByTestId('question-remove'))

    expect(screen.queryByTestId('question-card-new')).not.toBeInTheDocument()
    expect(removeMutate).not.toHaveBeenCalled()
  })

  // survey_guard: "every choice question needs at least two options". Not
  // checked here; sent, and answered at publish.
  test('a choice question with no options is still sent', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={[]} />)
    await userEvent.click(screen.getByTestId('question-add'))
    const card = screen.getByTestId('question-card-new')
    await userEvent.type(within(card).getByTestId('question-prompt-en'), 'Main crop?')
    await userEvent.click(within(card).getByTestId('question-save'))

    expect(saveMutateAsync.mock.calls[0][0]).toMatchObject({ kind: 'single_choice', options: [] })
  })
})

describe('editing a question', () => {
  test('saves in place under its id and position', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [card] = screen.getAllByTestId('question-card')

    await userEvent.clear(within(card).getByTestId('question-prompt-en'))
    await userEvent.type(within(card).getByTestId('question-prompt-en'), 'Main crop this season?')
    await userEvent.type(within(card).getByTestId('question-prompt-sw'), 'Zao kuu?')
    await userEvent.click(within(card).getByTestId('question-required'))
    await userEvent.click(within(card).getByTestId('question-save'))

    expect(saveMutateAsync).toHaveBeenCalledWith({
      id: 'q1',
      position: 1,
      kind: 'single_choice',
      prompt_en: 'Main crop this season?',
      prompt_sw: 'Zao kuu?',
      required: false,
      options: [
        { value: 'maize', label_en: 'Maize', label_sw: 'Mahindi' },
        { value: 'beans', label_en: 'Beans' },
      ],
    })
  })

  test('an edited label keeps its value; a new option takes one from its label', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [card] = screen.getAllByTestId('question-card')

    const labels = within(card).getAllByTestId('question-option-label-en')
    await userEvent.clear(labels[0])
    await userEvent.type(labels[0], 'Maize grain')
    await userEvent.click(within(card).getByTestId('question-add-option'))
    const added = within(card).getAllByTestId('question-option-label-en')[2]
    await userEvent.type(added, 'Sunflower')
    await userEvent.click(within(card).getByTestId('question-save'))

    expect(saveMutateAsync.mock.calls[0][0].options).toEqual([
      { value: 'maize', label_en: 'Maize grain', label_sw: 'Mahindi' },
      { value: 'beans', label_en: 'Beans' },
      { value: 'sunflower', label_en: 'Sunflower' },
    ])
  })

  test('an option can be removed', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [card] = screen.getAllByTestId('question-card')

    await userEvent.click(within(card).getAllByTestId('question-option-remove')[0])
    await userEvent.click(within(card).getByTestId('question-save'))

    expect(saveMutateAsync.mock.calls[0][0].options).toEqual([{ value: 'beans', label_en: 'Beans' }])
  })

  test('a kind without options saves none', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [card] = screen.getAllByTestId('question-card')

    await chooseKind(card, 'A number')
    expect(within(card).queryByTestId('question-option')).not.toBeInTheDocument()
    await userEvent.click(within(card).getByTestId('question-save'))

    expect(saveMutateAsync.mock.calls[0][0]).toMatchObject({ kind: 'number', options: [] })
  })

  test('the database refusal is shown on the question', () => {
    saveState.isError = true
    saveState.error = new Error('questions can only change while the survey is a draft')
    render(<QuestionsEditor surveyId={SURVEY} questions={[question()]} />)

    expect(within(screen.getByTestId('question-card')).getByTestId('error-state')).toHaveTextContent(
      'questions can only change while the survey is a draft',
    )
  })
})

describe('ordering and removing', () => {
  test('moving down swaps positions with the next question', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [first] = screen.getAllByTestId('question-card')

    await userEvent.click(within(first).getByTestId('question-move-down'))

    expect(swapMutate).toHaveBeenCalledWith({
      first: { id: 'q1', position: 1 },
      second: { id: 'q2', position: 2 },
    })
  })

  test('moving up swaps with the previous question', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [, second] = screen.getAllByTestId('question-card')

    await userEvent.click(within(second).getByTestId('question-move-up'))

    expect(swapMutate).toHaveBeenCalledWith({
      first: { id: 'q2', position: 2 },
      second: { id: 'q1', position: 1 },
    })
  })

  test('the first cannot move up and the last cannot move down', () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [first, last] = screen.getAllByTestId('question-card')

    expect(within(first).getByTestId('question-move-up')).toBeDisabled()
    expect(within(last).getByTestId('question-move-down')).toBeDisabled()
  })

  test('removing a question removes it by id', async () => {
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)
    const [first] = screen.getAllByTestId('question-card')

    await userEvent.click(within(first).getByTestId('question-remove'))

    expect(removeMutate).toHaveBeenCalledWith('q1')
  })

  test('a failed move or removal is shown', () => {
    removeState.isError = true
    removeState.error = new Error('questions can only change while the survey is a draft')
    render(<QuestionsEditor surveyId={SURVEY} questions={two()} />)

    expect(screen.getByTestId('questions-error')).toHaveTextContent('questions can only change while the survey is a draft')
  })
})

describe('the read-only list', () => {
  test('shows prompts, kinds and option labels with no controls', () => {
    render(<QuestionList questions={two()} />)

    const list = screen.getByTestId('question-list')
    expect(list).toHaveTextContent('What is your main crop?')
    expect(list).toHaveTextContent('One choice')
    expect(list).toHaveTextContent('Maize')
    expect(list).toHaveTextContent('A number')
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument()
    expect(screen.queryByTestId('question-save')).not.toBeInTheDocument()
  })

  test('no questions says so', () => {
    render(<QuestionList questions={[]} />)
    expect(screen.getByText('No questions yet.')).toBeInTheDocument()
  })
})
