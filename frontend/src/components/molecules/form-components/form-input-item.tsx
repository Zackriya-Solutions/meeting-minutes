import {
  FormControl,
  FormField,
  FormItem,
  FormMessage,
  FormLabel,
} from '@/components/ui/form';
import { Input } from '@/components/ui/input';
import { Eye, EyeOff } from 'lucide-react';
import { Control } from 'react-hook-form'; // Import Control type
import { Textarea } from '@/components/ui/textarea';

type IInpuItemProps = {
  name: string;
  placeholder?: string;
  control: Control<any>; // Add control prop of type Control
  type:
    | 'button'
    | 'checkbox'
    | 'color'
    | 'date'
    | 'datetime-local'
    | 'email'
    | 'file'
    | 'hidden'
    | 'image'
    | 'month'
    | 'number'
    | 'password'
    | 'radio'
    | 'range'
    | 'reset'
    | 'search'
    | 'submit'
    | 'tel'
    | 'text'
    | 'time'
    | 'url'
    | 'week'
    | (string & {});
  label?: string;
  formStyle?: string;
  formLabelStyle?: string;
  formControlStyle?: string;
  formMessageStyle?: string;
  defaultValue?: string | number;
  disabled?: boolean;
  togglePasswordVisibility?: () => void;
  showPassword?: boolean;
  accept?: string;
  inputStyle?: string;
};

export const FormInputItem = ({
  name,
  showPassword,
  togglePasswordVisibility,
  placeholder,
  control,
  label,
  type,
  formStyle,
  formLabelStyle,
  formControlStyle,
  formMessageStyle,
  defaultValue,
  disabled,
  accept,
  inputStyle,
}: IInpuItemProps) => {
  return (
    <div>
      <FormField
        control={control} // Use the control prop passed from the parent
        name={name}
        defaultValue={defaultValue}
        disabled={disabled}
        render={({ field }) => (
          <FormItem
          // className={formStyle}
          >
            <div className={formStyle}>
              <FormLabel className={formLabelStyle}>{label}</FormLabel>
              <FormControl className={formControlStyle}>
                {type === 'password' ? (
                  <div className="relative">
                    <Input
                      type={showPassword ? 'text' : 'password'}
                      placeholder={placeholder}
                      {...field}
                      accept={accept}
                      className={inputStyle}
                    />
                    <div className="absolute inset-y-0 right-0 pr-3 flex items-center text-muted-foreground cursor-pointer">
                      {showPassword ? (
                        <EyeOff
                          className="h-6 w-6"
                          onClick={togglePasswordVisibility}
                        />
                      ) : (
                        <Eye
                          className="h-6 w-6"
                          onClick={togglePasswordVisibility}
                        />
                      )}
                    </div>
                  </div>
                ) : type === 'textarea' ? (
                  <Textarea
                    placeholder={placeholder}
                    {...field}
                    className={inputStyle}
                    rows={4}
                  />
                ) : (
                  <Input
                    placeholder={placeholder}
                    {...field}
                    type={type}
                    accept={accept}
                    className={inputStyle}
                  />
                )}
              </FormControl>
            </div>
            <FormMessage className={formMessageStyle} />
          </FormItem>
        )}
      />
    </div>
  );
};
