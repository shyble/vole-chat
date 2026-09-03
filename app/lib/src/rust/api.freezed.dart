// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$SendVerdict {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendVerdict);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SendVerdict()';
}


}

/// @nodoc
class $SendVerdictCopyWith<$Res>  {
$SendVerdictCopyWith(SendVerdict _, $Res Function(SendVerdict) __);
}


/// Adds pattern-matching-related methods to [SendVerdict].
extension SendVerdictPatterns on SendVerdict {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( SendVerdict_Delivered value)?  delivered,TResult Function( SendVerdict_Held value)?  held,TResult Function( SendVerdict_Refused value)?  refused,TResult Function( SendVerdict_Legacy value)?  legacy,required TResult orElse(),}){
final _that = this;
switch (_that) {
case SendVerdict_Delivered() when delivered != null:
return delivered(_that);case SendVerdict_Held() when held != null:
return held(_that);case SendVerdict_Refused() when refused != null:
return refused(_that);case SendVerdict_Legacy() when legacy != null:
return legacy(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( SendVerdict_Delivered value)  delivered,required TResult Function( SendVerdict_Held value)  held,required TResult Function( SendVerdict_Refused value)  refused,required TResult Function( SendVerdict_Legacy value)  legacy,}){
final _that = this;
switch (_that) {
case SendVerdict_Delivered():
return delivered(_that);case SendVerdict_Held():
return held(_that);case SendVerdict_Refused():
return refused(_that);case SendVerdict_Legacy():
return legacy(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( SendVerdict_Delivered value)?  delivered,TResult? Function( SendVerdict_Held value)?  held,TResult? Function( SendVerdict_Refused value)?  refused,TResult? Function( SendVerdict_Legacy value)?  legacy,}){
final _that = this;
switch (_that) {
case SendVerdict_Delivered() when delivered != null:
return delivered(_that);case SendVerdict_Held() when held != null:
return held(_that);case SendVerdict_Refused() when refused != null:
return refused(_that);case SendVerdict_Legacy() when legacy != null:
return legacy(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  delivered,TResult Function()?  held,TResult Function( String reason)?  refused,TResult Function()?  legacy,required TResult orElse(),}) {final _that = this;
switch (_that) {
case SendVerdict_Delivered() when delivered != null:
return delivered();case SendVerdict_Held() when held != null:
return held();case SendVerdict_Refused() when refused != null:
return refused(_that.reason);case SendVerdict_Legacy() when legacy != null:
return legacy();case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  delivered,required TResult Function()  held,required TResult Function( String reason)  refused,required TResult Function()  legacy,}) {final _that = this;
switch (_that) {
case SendVerdict_Delivered():
return delivered();case SendVerdict_Held():
return held();case SendVerdict_Refused():
return refused(_that.reason);case SendVerdict_Legacy():
return legacy();}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  delivered,TResult? Function()?  held,TResult? Function( String reason)?  refused,TResult? Function()?  legacy,}) {final _that = this;
switch (_that) {
case SendVerdict_Delivered() when delivered != null:
return delivered();case SendVerdict_Held() when held != null:
return held();case SendVerdict_Refused() when refused != null:
return refused(_that.reason);case SendVerdict_Legacy() when legacy != null:
return legacy();case _:
  return null;

}
}

}

/// @nodoc


class SendVerdict_Delivered extends SendVerdict {
  const SendVerdict_Delivered(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendVerdict_Delivered);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SendVerdict.delivered()';
}


}




/// @nodoc


class SendVerdict_Held extends SendVerdict {
  const SendVerdict_Held(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendVerdict_Held);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SendVerdict.held()';
}


}




/// @nodoc


class SendVerdict_Refused extends SendVerdict {
  const SendVerdict_Refused({required this.reason}): super._();
  

 final  String reason;

/// Create a copy of SendVerdict
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SendVerdict_RefusedCopyWith<SendVerdict_Refused> get copyWith => _$SendVerdict_RefusedCopyWithImpl<SendVerdict_Refused>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendVerdict_Refused&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,reason);

@override
String toString() {
  return 'SendVerdict.refused(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $SendVerdict_RefusedCopyWith<$Res> implements $SendVerdictCopyWith<$Res> {
  factory $SendVerdict_RefusedCopyWith(SendVerdict_Refused value, $Res Function(SendVerdict_Refused) _then) = _$SendVerdict_RefusedCopyWithImpl;
@useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$SendVerdict_RefusedCopyWithImpl<$Res>
    implements $SendVerdict_RefusedCopyWith<$Res> {
  _$SendVerdict_RefusedCopyWithImpl(this._self, this._then);

  final SendVerdict_Refused _self;
  final $Res Function(SendVerdict_Refused) _then;

/// Create a copy of SendVerdict
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,}) {
  return _then(SendVerdict_Refused(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class SendVerdict_Legacy extends SendVerdict {
  const SendVerdict_Legacy(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SendVerdict_Legacy);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SendVerdict.legacy()';
}


}




/// @nodoc
mixin _$SessionEvent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'SessionEvent()';
}


}

/// @nodoc
class $SessionEventCopyWith<$Res>  {
$SessionEventCopyWith(SessionEvent _, $Res Function(SessionEvent) __);
}


/// Adds pattern-matching-related methods to [SessionEvent].
extension SessionEventPatterns on SessionEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( SessionEvent_Connected value)?  connected,TResult Function( SessionEvent_Disconnected value)?  disconnected,TResult Function( SessionEvent_Roster value)?  roster,TResult Function( SessionEvent_Chat value)?  chat,TResult Function( SessionEvent_ConnectRequest value)?  connectRequest,TResult Function( SessionEvent_ConnectAccepted value)?  connectAccepted,TResult Function( SessionEvent_ConnectDenied value)?  connectDenied,TResult Function( SessionEvent_Pending value)?  pending,required TResult orElse(),}){
final _that = this;
switch (_that) {
case SessionEvent_Connected() when connected != null:
return connected(_that);case SessionEvent_Disconnected() when disconnected != null:
return disconnected(_that);case SessionEvent_Roster() when roster != null:
return roster(_that);case SessionEvent_Chat() when chat != null:
return chat(_that);case SessionEvent_ConnectRequest() when connectRequest != null:
return connectRequest(_that);case SessionEvent_ConnectAccepted() when connectAccepted != null:
return connectAccepted(_that);case SessionEvent_ConnectDenied() when connectDenied != null:
return connectDenied(_that);case SessionEvent_Pending() when pending != null:
return pending(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( SessionEvent_Connected value)  connected,required TResult Function( SessionEvent_Disconnected value)  disconnected,required TResult Function( SessionEvent_Roster value)  roster,required TResult Function( SessionEvent_Chat value)  chat,required TResult Function( SessionEvent_ConnectRequest value)  connectRequest,required TResult Function( SessionEvent_ConnectAccepted value)  connectAccepted,required TResult Function( SessionEvent_ConnectDenied value)  connectDenied,required TResult Function( SessionEvent_Pending value)  pending,}){
final _that = this;
switch (_that) {
case SessionEvent_Connected():
return connected(_that);case SessionEvent_Disconnected():
return disconnected(_that);case SessionEvent_Roster():
return roster(_that);case SessionEvent_Chat():
return chat(_that);case SessionEvent_ConnectRequest():
return connectRequest(_that);case SessionEvent_ConnectAccepted():
return connectAccepted(_that);case SessionEvent_ConnectDenied():
return connectDenied(_that);case SessionEvent_Pending():
return pending(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( SessionEvent_Connected value)?  connected,TResult? Function( SessionEvent_Disconnected value)?  disconnected,TResult? Function( SessionEvent_Roster value)?  roster,TResult? Function( SessionEvent_Chat value)?  chat,TResult? Function( SessionEvent_ConnectRequest value)?  connectRequest,TResult? Function( SessionEvent_ConnectAccepted value)?  connectAccepted,TResult? Function( SessionEvent_ConnectDenied value)?  connectDenied,TResult? Function( SessionEvent_Pending value)?  pending,}){
final _that = this;
switch (_that) {
case SessionEvent_Connected() when connected != null:
return connected(_that);case SessionEvent_Disconnected() when disconnected != null:
return disconnected(_that);case SessionEvent_Roster() when roster != null:
return roster(_that);case SessionEvent_Chat() when chat != null:
return chat(_that);case SessionEvent_ConnectRequest() when connectRequest != null:
return connectRequest(_that);case SessionEvent_ConnectAccepted() when connectAccepted != null:
return connectAccepted(_that);case SessionEvent_ConnectDenied() when connectDenied != null:
return connectDenied(_that);case SessionEvent_Pending() when pending != null:
return pending(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String hubName)?  connected,TResult Function( String reason)?  disconnected,TResult Function( List<MemberInfo> members)?  roster,TResult Function( String from,  String fromName,  String text,  PlatformInt64 sentAt,  String id)?  chat,TResult Function( String from,  String fromName,  String? note)?  connectRequest,TResult Function( String from,  String fromName)?  connectAccepted,TResult Function( String from,  String fromName)?  connectDenied,TResult Function( List<NoticeInfo> notices)?  pending,required TResult orElse(),}) {final _that = this;
switch (_that) {
case SessionEvent_Connected() when connected != null:
return connected(_that.hubName);case SessionEvent_Disconnected() when disconnected != null:
return disconnected(_that.reason);case SessionEvent_Roster() when roster != null:
return roster(_that.members);case SessionEvent_Chat() when chat != null:
return chat(_that.from,_that.fromName,_that.text,_that.sentAt,_that.id);case SessionEvent_ConnectRequest() when connectRequest != null:
return connectRequest(_that.from,_that.fromName,_that.note);case SessionEvent_ConnectAccepted() when connectAccepted != null:
return connectAccepted(_that.from,_that.fromName);case SessionEvent_ConnectDenied() when connectDenied != null:
return connectDenied(_that.from,_that.fromName);case SessionEvent_Pending() when pending != null:
return pending(_that.notices);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String hubName)  connected,required TResult Function( String reason)  disconnected,required TResult Function( List<MemberInfo> members)  roster,required TResult Function( String from,  String fromName,  String text,  PlatformInt64 sentAt,  String id)  chat,required TResult Function( String from,  String fromName,  String? note)  connectRequest,required TResult Function( String from,  String fromName)  connectAccepted,required TResult Function( String from,  String fromName)  connectDenied,required TResult Function( List<NoticeInfo> notices)  pending,}) {final _that = this;
switch (_that) {
case SessionEvent_Connected():
return connected(_that.hubName);case SessionEvent_Disconnected():
return disconnected(_that.reason);case SessionEvent_Roster():
return roster(_that.members);case SessionEvent_Chat():
return chat(_that.from,_that.fromName,_that.text,_that.sentAt,_that.id);case SessionEvent_ConnectRequest():
return connectRequest(_that.from,_that.fromName,_that.note);case SessionEvent_ConnectAccepted():
return connectAccepted(_that.from,_that.fromName);case SessionEvent_ConnectDenied():
return connectDenied(_that.from,_that.fromName);case SessionEvent_Pending():
return pending(_that.notices);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String hubName)?  connected,TResult? Function( String reason)?  disconnected,TResult? Function( List<MemberInfo> members)?  roster,TResult? Function( String from,  String fromName,  String text,  PlatformInt64 sentAt,  String id)?  chat,TResult? Function( String from,  String fromName,  String? note)?  connectRequest,TResult? Function( String from,  String fromName)?  connectAccepted,TResult? Function( String from,  String fromName)?  connectDenied,TResult? Function( List<NoticeInfo> notices)?  pending,}) {final _that = this;
switch (_that) {
case SessionEvent_Connected() when connected != null:
return connected(_that.hubName);case SessionEvent_Disconnected() when disconnected != null:
return disconnected(_that.reason);case SessionEvent_Roster() when roster != null:
return roster(_that.members);case SessionEvent_Chat() when chat != null:
return chat(_that.from,_that.fromName,_that.text,_that.sentAt,_that.id);case SessionEvent_ConnectRequest() when connectRequest != null:
return connectRequest(_that.from,_that.fromName,_that.note);case SessionEvent_ConnectAccepted() when connectAccepted != null:
return connectAccepted(_that.from,_that.fromName);case SessionEvent_ConnectDenied() when connectDenied != null:
return connectDenied(_that.from,_that.fromName);case SessionEvent_Pending() when pending != null:
return pending(_that.notices);case _:
  return null;

}
}

}

/// @nodoc


class SessionEvent_Connected extends SessionEvent {
  const SessionEvent_Connected({required this.hubName}): super._();
  

 final  String hubName;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_ConnectedCopyWith<SessionEvent_Connected> get copyWith => _$SessionEvent_ConnectedCopyWithImpl<SessionEvent_Connected>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_Connected&&(identical(other.hubName, hubName) || other.hubName == hubName));
}


@override
int get hashCode => Object.hash(runtimeType,hubName);

@override
String toString() {
  return 'SessionEvent.connected(hubName: $hubName)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_ConnectedCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_ConnectedCopyWith(SessionEvent_Connected value, $Res Function(SessionEvent_Connected) _then) = _$SessionEvent_ConnectedCopyWithImpl;
@useResult
$Res call({
 String hubName
});




}
/// @nodoc
class _$SessionEvent_ConnectedCopyWithImpl<$Res>
    implements $SessionEvent_ConnectedCopyWith<$Res> {
  _$SessionEvent_ConnectedCopyWithImpl(this._self, this._then);

  final SessionEvent_Connected _self;
  final $Res Function(SessionEvent_Connected) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? hubName = null,}) {
  return _then(SessionEvent_Connected(
hubName: null == hubName ? _self.hubName : hubName // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class SessionEvent_Disconnected extends SessionEvent {
  const SessionEvent_Disconnected({required this.reason}): super._();
  

 final  String reason;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_DisconnectedCopyWith<SessionEvent_Disconnected> get copyWith => _$SessionEvent_DisconnectedCopyWithImpl<SessionEvent_Disconnected>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_Disconnected&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,reason);

@override
String toString() {
  return 'SessionEvent.disconnected(reason: $reason)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_DisconnectedCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_DisconnectedCopyWith(SessionEvent_Disconnected value, $Res Function(SessionEvent_Disconnected) _then) = _$SessionEvent_DisconnectedCopyWithImpl;
@useResult
$Res call({
 String reason
});




}
/// @nodoc
class _$SessionEvent_DisconnectedCopyWithImpl<$Res>
    implements $SessionEvent_DisconnectedCopyWith<$Res> {
  _$SessionEvent_DisconnectedCopyWithImpl(this._self, this._then);

  final SessionEvent_Disconnected _self;
  final $Res Function(SessionEvent_Disconnected) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,}) {
  return _then(SessionEvent_Disconnected(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class SessionEvent_Roster extends SessionEvent {
  const SessionEvent_Roster({required final  List<MemberInfo> members}): _members = members,super._();
  

 final  List<MemberInfo> _members;
 List<MemberInfo> get members {
  if (_members is EqualUnmodifiableListView) return _members;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_members);
}


/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_RosterCopyWith<SessionEvent_Roster> get copyWith => _$SessionEvent_RosterCopyWithImpl<SessionEvent_Roster>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_Roster&&const DeepCollectionEquality().equals(other._members, _members));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_members));

@override
String toString() {
  return 'SessionEvent.roster(members: $members)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_RosterCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_RosterCopyWith(SessionEvent_Roster value, $Res Function(SessionEvent_Roster) _then) = _$SessionEvent_RosterCopyWithImpl;
@useResult
$Res call({
 List<MemberInfo> members
});




}
/// @nodoc
class _$SessionEvent_RosterCopyWithImpl<$Res>
    implements $SessionEvent_RosterCopyWith<$Res> {
  _$SessionEvent_RosterCopyWithImpl(this._self, this._then);

  final SessionEvent_Roster _self;
  final $Res Function(SessionEvent_Roster) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? members = null,}) {
  return _then(SessionEvent_Roster(
members: null == members ? _self._members : members // ignore: cast_nullable_to_non_nullable
as List<MemberInfo>,
  ));
}


}

/// @nodoc


class SessionEvent_Chat extends SessionEvent {
  const SessionEvent_Chat({required this.from, required this.fromName, required this.text, required this.sentAt, required this.id}): super._();
  

 final  String from;
 final  String fromName;
 final  String text;
 final  PlatformInt64 sentAt;
 final  String id;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_ChatCopyWith<SessionEvent_Chat> get copyWith => _$SessionEvent_ChatCopyWithImpl<SessionEvent_Chat>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_Chat&&(identical(other.from, from) || other.from == from)&&(identical(other.fromName, fromName) || other.fromName == fromName)&&(identical(other.text, text) || other.text == text)&&(identical(other.sentAt, sentAt) || other.sentAt == sentAt)&&(identical(other.id, id) || other.id == id));
}


@override
int get hashCode => Object.hash(runtimeType,from,fromName,text,sentAt,id);

@override
String toString() {
  return 'SessionEvent.chat(from: $from, fromName: $fromName, text: $text, sentAt: $sentAt, id: $id)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_ChatCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_ChatCopyWith(SessionEvent_Chat value, $Res Function(SessionEvent_Chat) _then) = _$SessionEvent_ChatCopyWithImpl;
@useResult
$Res call({
 String from, String fromName, String text, PlatformInt64 sentAt, String id
});




}
/// @nodoc
class _$SessionEvent_ChatCopyWithImpl<$Res>
    implements $SessionEvent_ChatCopyWith<$Res> {
  _$SessionEvent_ChatCopyWithImpl(this._self, this._then);

  final SessionEvent_Chat _self;
  final $Res Function(SessionEvent_Chat) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? from = null,Object? fromName = null,Object? text = null,Object? sentAt = null,Object? id = null,}) {
  return _then(SessionEvent_Chat(
from: null == from ? _self.from : from // ignore: cast_nullable_to_non_nullable
as String,fromName: null == fromName ? _self.fromName : fromName // ignore: cast_nullable_to_non_nullable
as String,text: null == text ? _self.text : text // ignore: cast_nullable_to_non_nullable
as String,sentAt: null == sentAt ? _self.sentAt : sentAt // ignore: cast_nullable_to_non_nullable
as PlatformInt64,id: null == id ? _self.id : id // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class SessionEvent_ConnectRequest extends SessionEvent {
  const SessionEvent_ConnectRequest({required this.from, required this.fromName, this.note}): super._();
  

 final  String from;
 final  String fromName;
 final  String? note;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_ConnectRequestCopyWith<SessionEvent_ConnectRequest> get copyWith => _$SessionEvent_ConnectRequestCopyWithImpl<SessionEvent_ConnectRequest>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_ConnectRequest&&(identical(other.from, from) || other.from == from)&&(identical(other.fromName, fromName) || other.fromName == fromName)&&(identical(other.note, note) || other.note == note));
}


@override
int get hashCode => Object.hash(runtimeType,from,fromName,note);

@override
String toString() {
  return 'SessionEvent.connectRequest(from: $from, fromName: $fromName, note: $note)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_ConnectRequestCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_ConnectRequestCopyWith(SessionEvent_ConnectRequest value, $Res Function(SessionEvent_ConnectRequest) _then) = _$SessionEvent_ConnectRequestCopyWithImpl;
@useResult
$Res call({
 String from, String fromName, String? note
});




}
/// @nodoc
class _$SessionEvent_ConnectRequestCopyWithImpl<$Res>
    implements $SessionEvent_ConnectRequestCopyWith<$Res> {
  _$SessionEvent_ConnectRequestCopyWithImpl(this._self, this._then);

  final SessionEvent_ConnectRequest _self;
  final $Res Function(SessionEvent_ConnectRequest) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? from = null,Object? fromName = null,Object? note = freezed,}) {
  return _then(SessionEvent_ConnectRequest(
from: null == from ? _self.from : from // ignore: cast_nullable_to_non_nullable
as String,fromName: null == fromName ? _self.fromName : fromName // ignore: cast_nullable_to_non_nullable
as String,note: freezed == note ? _self.note : note // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

/// @nodoc


class SessionEvent_ConnectAccepted extends SessionEvent {
  const SessionEvent_ConnectAccepted({required this.from, required this.fromName}): super._();
  

 final  String from;
 final  String fromName;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_ConnectAcceptedCopyWith<SessionEvent_ConnectAccepted> get copyWith => _$SessionEvent_ConnectAcceptedCopyWithImpl<SessionEvent_ConnectAccepted>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_ConnectAccepted&&(identical(other.from, from) || other.from == from)&&(identical(other.fromName, fromName) || other.fromName == fromName));
}


@override
int get hashCode => Object.hash(runtimeType,from,fromName);

@override
String toString() {
  return 'SessionEvent.connectAccepted(from: $from, fromName: $fromName)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_ConnectAcceptedCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_ConnectAcceptedCopyWith(SessionEvent_ConnectAccepted value, $Res Function(SessionEvent_ConnectAccepted) _then) = _$SessionEvent_ConnectAcceptedCopyWithImpl;
@useResult
$Res call({
 String from, String fromName
});




}
/// @nodoc
class _$SessionEvent_ConnectAcceptedCopyWithImpl<$Res>
    implements $SessionEvent_ConnectAcceptedCopyWith<$Res> {
  _$SessionEvent_ConnectAcceptedCopyWithImpl(this._self, this._then);

  final SessionEvent_ConnectAccepted _self;
  final $Res Function(SessionEvent_ConnectAccepted) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? from = null,Object? fromName = null,}) {
  return _then(SessionEvent_ConnectAccepted(
from: null == from ? _self.from : from // ignore: cast_nullable_to_non_nullable
as String,fromName: null == fromName ? _self.fromName : fromName // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class SessionEvent_ConnectDenied extends SessionEvent {
  const SessionEvent_ConnectDenied({required this.from, required this.fromName}): super._();
  

 final  String from;
 final  String fromName;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_ConnectDeniedCopyWith<SessionEvent_ConnectDenied> get copyWith => _$SessionEvent_ConnectDeniedCopyWithImpl<SessionEvent_ConnectDenied>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_ConnectDenied&&(identical(other.from, from) || other.from == from)&&(identical(other.fromName, fromName) || other.fromName == fromName));
}


@override
int get hashCode => Object.hash(runtimeType,from,fromName);

@override
String toString() {
  return 'SessionEvent.connectDenied(from: $from, fromName: $fromName)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_ConnectDeniedCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_ConnectDeniedCopyWith(SessionEvent_ConnectDenied value, $Res Function(SessionEvent_ConnectDenied) _then) = _$SessionEvent_ConnectDeniedCopyWithImpl;
@useResult
$Res call({
 String from, String fromName
});




}
/// @nodoc
class _$SessionEvent_ConnectDeniedCopyWithImpl<$Res>
    implements $SessionEvent_ConnectDeniedCopyWith<$Res> {
  _$SessionEvent_ConnectDeniedCopyWithImpl(this._self, this._then);

  final SessionEvent_ConnectDenied _self;
  final $Res Function(SessionEvent_ConnectDenied) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? from = null,Object? fromName = null,}) {
  return _then(SessionEvent_ConnectDenied(
from: null == from ? _self.from : from // ignore: cast_nullable_to_non_nullable
as String,fromName: null == fromName ? _self.fromName : fromName // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class SessionEvent_Pending extends SessionEvent {
  const SessionEvent_Pending({required final  List<NoticeInfo> notices}): _notices = notices,super._();
  

 final  List<NoticeInfo> _notices;
 List<NoticeInfo> get notices {
  if (_notices is EqualUnmodifiableListView) return _notices;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_notices);
}


/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$SessionEvent_PendingCopyWith<SessionEvent_Pending> get copyWith => _$SessionEvent_PendingCopyWithImpl<SessionEvent_Pending>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is SessionEvent_Pending&&const DeepCollectionEquality().equals(other._notices, _notices));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_notices));

@override
String toString() {
  return 'SessionEvent.pending(notices: $notices)';
}


}

/// @nodoc
abstract mixin class $SessionEvent_PendingCopyWith<$Res> implements $SessionEventCopyWith<$Res> {
  factory $SessionEvent_PendingCopyWith(SessionEvent_Pending value, $Res Function(SessionEvent_Pending) _then) = _$SessionEvent_PendingCopyWithImpl;
@useResult
$Res call({
 List<NoticeInfo> notices
});




}
/// @nodoc
class _$SessionEvent_PendingCopyWithImpl<$Res>
    implements $SessionEvent_PendingCopyWith<$Res> {
  _$SessionEvent_PendingCopyWithImpl(this._self, this._then);

  final SessionEvent_Pending _self;
  final $Res Function(SessionEvent_Pending) _then;

/// Create a copy of SessionEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? notices = null,}) {
  return _then(SessionEvent_Pending(
notices: null == notices ? _self._notices : notices // ignore: cast_nullable_to_non_nullable
as List<NoticeInfo>,
  ));
}


}

// dart format on
