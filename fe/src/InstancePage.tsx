// An instance's page (#/instances/<id>, signed in): the header with the id,
// the engine and the state, read once from /api/instances, then the Server
// section (fe/briefs/01-server.md, section 2.4).
import { useEffect, useState } from 'react'
import { errorText, readInstances, type Answer } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import ServerSection, { InstanceNotFound } from './ServerSection.tsx'
import { StateShape, stateLabel } from './StateBadge.tsx'
import { t } from './strings.ts'

export default function InstancePage({
  id,
  canConfigure,
  onSignedOut,
}: {
  id: string
  canConfigure: boolean
  onSignedOut: () => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  useEffect(() => {
    let live = true
    readInstances().then((a) => {
      if (live) setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [])

  return (
    <>
      <PageHeading text={id} />
      <Body
        id={id}
        answer={answer}
        canConfigure={canConfigure}
        onSignedOut={onSignedOut}
      />
    </>
  )
}

function Body({
  id,
  answer,
  canConfigure,
  onSignedOut,
}: {
  id: string
  answer: Answer | undefined
  canConfigure: boolean
  onSignedOut: () => void
}) {
  if (answer === undefined) {
    return <p role="status">{t('loading')}</p>
  }
  if (!Array.isArray(answer)) {
    return <p>{errorText(answer.error)}</p>
  }
  const instance = answer.find((i) => i.id === id)
  if (instance === undefined) {
    return <InstanceNotFound id={id} />
  }
  return (
    <>
      <div className="instance-header">
        <p className="engine">{t('instance-engine', instance.engine)}</p>
        <p>
          <StateShape state={instance.state} />
          {t('instance-state', stateLabel(instance.state))}
        </p>
      </div>
      <ServerSection
        id={id}
        canConfigure={canConfigure}
        onSignedOut={onSignedOut}
      />
    </>
  )
}
