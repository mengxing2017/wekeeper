import { SharedSendTasks } from './shared-send-tasks'

type Props = { admin: boolean; onNavigate?: (view: string) => void }
export function MobileExecutionTasks(props: Props) {
  return <SharedSendTasks key={`${props.admin}:${props.admin}`} {...props} />
}
