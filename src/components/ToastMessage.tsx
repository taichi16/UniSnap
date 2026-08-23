interface ToastMessageProps {
  message: string;
}

export default function ToastMessage({ message }: ToastMessageProps) {
  return <div className="toast">{message}</div>;
}
